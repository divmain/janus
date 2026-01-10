pub mod parser;
pub mod types;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::error::{JanusError, Result};
use crate::plan::parser::parse_plan_content;
use crate::plan::types::{Phase, PhaseStatus, PlanMetadata, PlanStatus};
use crate::types::{PLANS_DIR, TicketMetadata, TicketStatus};

// Re-export importable plan types for external use
pub use crate::plan::types::{
    ImportValidationError, ImportablePhase, ImportablePlan, ImportableTask,
};

// Re-export parser functions for plan import
pub use crate::plan::parser::{
    ACCEPTANCE_CRITERIA_ALIASES, DESIGN_SECTION_NAME, IMPLEMENTATION_SECTION_NAME, PHASE_PATTERN,
    is_completed_task, is_phase_header, is_section_alias, parse_importable_plan,
};

/// Find all plan files in the plans directory
fn find_plans() -> Vec<String> {
    fs::read_dir(PLANS_DIR)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if name.ends_with(".md") {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Find a plan file by partial ID
pub fn find_plan_by_id(partial_id: &str) -> Result<PathBuf> {
    let files = find_plans();

    // Check for exact match first
    let exact_name = format!("{}.md", partial_id);
    if files.iter().any(|f| f == &exact_name) {
        return Ok(PathBuf::from(PLANS_DIR).join(&exact_name));
    }

    // Then check for partial matches
    let matches: Vec<_> = files.iter().filter(|f| f.contains(partial_id)).collect();

    match matches.len() {
        0 => Err(JanusError::PlanNotFound(partial_id.to_string())),
        1 => Ok(PathBuf::from(PLANS_DIR).join(matches[0])),
        _ => Err(JanusError::AmbiguousPlanId(partial_id.to_string())),
    }
}

/// A plan handle for reading and writing plan files
pub struct Plan {
    pub file_path: PathBuf,
    pub id: String,
}

impl Plan {
    /// Find a plan by its (partial) ID
    pub fn find(partial_id: &str) -> Result<Self> {
        let file_path = find_plan_by_id(partial_id)?;
        Ok(Plan::new(file_path))
    }

    /// Create a plan handle for a given file path
    pub fn new(file_path: PathBuf) -> Self {
        let id = file_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        Plan { file_path, id }
    }

    /// Create a plan handle for a new plan with the given ID
    pub fn with_id(id: &str) -> Self {
        let file_path = PathBuf::from(PLANS_DIR).join(format!("{}.md", id));
        Plan {
            file_path,
            id: id.to_string(),
        }
    }

    /// Check if the plan file exists
    pub fn exists(&self) -> bool {
        self.file_path.exists()
    }

    /// Read and parse the plan's metadata
    ///
    /// Parses the full plan file including YAML frontmatter, title, description,
    /// acceptance criteria, phases/tickets, and free-form sections.
    pub fn read(&self) -> Result<PlanMetadata> {
        let content = fs::read_to_string(&self.file_path)?;
        let mut metadata = parse_plan_content(&content)?;
        metadata.file_path = Some(self.file_path.clone());
        Ok(metadata)
    }

    /// Read the raw content of the plan file
    pub fn read_content(&self) -> Result<String> {
        Ok(fs::read_to_string(&self.file_path)?)
    }

    /// Write content to the plan file
    pub fn write(&self, content: &str) -> Result<()> {
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.file_path, content)?;
        Ok(())
    }

    /// Delete the plan file
    pub fn delete(&self) -> Result<()> {
        if self.file_path.exists() {
            fs::remove_file(&self.file_path)?;
        }
        Ok(())
    }

    /// Compute the status of this plan based on its tickets
    ///
    /// This requires a map of all tickets to look up their statuses.
    pub fn compute_status(
        &self,
        ticket_map: &HashMap<String, TicketMetadata>,
    ) -> Result<PlanStatus> {
        let metadata = self.read()?;
        Ok(compute_plan_status(&metadata, ticket_map))
    }

    /// Compute the status of a specific phase
    pub fn compute_phase_status(
        &self,
        phase: &Phase,
        ticket_map: &HashMap<String, TicketMetadata>,
    ) -> PhaseStatus {
        compute_phase_status_impl(phase, ticket_map)
    }

    /// Compute the status of all phases in this plan
    ///
    /// Returns a vector of `PhaseStatus` for each phase in document order.
    /// For simple plans (no phases), returns an empty vector.
    pub fn compute_all_phase_statuses(
        &self,
        ticket_map: &HashMap<String, TicketMetadata>,
    ) -> Result<Vec<PhaseStatus>> {
        let metadata = self.read()?;
        Ok(compute_all_phase_statuses(&metadata, ticket_map))
    }
}

/// Compute the status of a plan based on its constituent tickets.
///
/// Status computation rules:
/// 1. If all tickets are `complete` → plan status is `complete`
/// 2. If all tickets are `cancelled` → plan status is `cancelled`
/// 3. If all tickets are `complete` or `cancelled` (mixed) → plan status is `complete`
/// 4. If all tickets are `new` or `next` (not started) → plan status is `new`
/// 5. Otherwise (some started, some not started) → plan status is `in_progress`
///
/// Missing tickets are skipped with a warning printed to stderr.
pub fn compute_plan_status(
    metadata: &PlanMetadata,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> PlanStatus {
    let all_ticket_ids = metadata.all_tickets();
    let total_count = all_ticket_ids.len();

    if total_count == 0 {
        return PlanStatus {
            status: TicketStatus::New,
            completed_count: 0,
            total_count: 0,
        };
    }

    // Collect statuses of all referenced tickets, warning about missing ones
    let mut statuses: Vec<TicketStatus> = Vec::new();
    for id in &all_ticket_ids {
        if let Some(ticket) = ticket_map.get(*id) {
            if let Some(status) = ticket.status {
                statuses.push(status);
            }
        } else {
            eprintln!(
                "Warning: ticket '{}' referenced in plan '{}' not found",
                id,
                metadata.id.as_deref().unwrap_or("unknown")
            );
        }
    }

    let completed_count = statuses
        .iter()
        .filter(|s| **s == TicketStatus::Complete)
        .count();

    let status = compute_aggregate_status(&statuses);

    PlanStatus {
        status,
        completed_count,
        total_count,
    }
}

/// Compute the status of a single phase
///
/// Missing tickets are skipped with a warning printed to stderr.
fn compute_phase_status_impl(
    phase: &Phase,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> PhaseStatus {
    compute_phase_status_impl_inner(phase, ticket_map, true)
}

/// Compute the status of a single phase (internal implementation)
///
/// # Arguments
/// * `phase` - The phase to compute status for
/// * `ticket_map` - Map of ticket IDs to metadata
/// * `warn_missing` - If true, print warnings for missing tickets to stderr
fn compute_phase_status_impl_inner(
    phase: &Phase,
    ticket_map: &HashMap<String, TicketMetadata>,
    warn_missing: bool,
) -> PhaseStatus {
    let total_count = phase.tickets.len();

    if total_count == 0 {
        return PhaseStatus {
            phase_number: phase.number.clone(),
            phase_name: phase.name.clone(),
            status: TicketStatus::New,
            completed_count: 0,
            total_count: 0,
        };
    }

    // Collect statuses of all referenced tickets, warning about missing ones
    let mut statuses: Vec<TicketStatus> = Vec::new();
    for id in &phase.tickets {
        if let Some(ticket) = ticket_map.get(id) {
            if let Some(status) = ticket.status {
                statuses.push(status);
            }
        } else if warn_missing {
            eprintln!(
                "Warning: ticket '{}' referenced in phase '{}' not found",
                id, phase.name
            );
        }
    }

    let completed_count = statuses
        .iter()
        .filter(|s| **s == TicketStatus::Complete)
        .count();

    let status = compute_aggregate_status(&statuses);

    PhaseStatus {
        phase_number: phase.number.clone(),
        phase_name: phase.name.clone(),
        status,
        completed_count,
        total_count,
    }
}

/// Compute the status of all phases in a plan.
///
/// Returns a vector of `PhaseStatus` for each phase in document order.
/// For simple plans (no phases), returns an empty vector.
/// Missing tickets are warned about to stderr.
pub fn compute_all_phase_statuses(
    metadata: &PlanMetadata,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> Vec<PhaseStatus> {
    // Use the inner function without warnings since compute_plan_status
    // already warns about missing tickets at the plan level
    metadata
        .phases()
        .iter()
        .map(|phase| compute_phase_status_impl_inner(phase, ticket_map, false))
        .collect()
}

/// Compute aggregate status from a list of ticket statuses.
///
/// Rules:
/// 1. If all tickets are `complete` → `complete`
/// 2. If all tickets are `cancelled` → `cancelled`
/// 3. If all tickets are `complete` or `cancelled` (mixed) → `complete`
/// 4. If all tickets are `new` or `next` (not started) → `new`
/// 5. Otherwise (some started, some not started) → `in_progress`
fn compute_aggregate_status(statuses: &[TicketStatus]) -> TicketStatus {
    if statuses.is_empty() {
        return TicketStatus::New;
    }

    let all_complete = statuses.iter().all(|s| *s == TicketStatus::Complete);
    let all_cancelled = statuses.iter().all(|s| *s == TicketStatus::Cancelled);
    let all_finished = statuses
        .iter()
        .all(|s| *s == TicketStatus::Complete || *s == TicketStatus::Cancelled);
    let all_not_started = statuses
        .iter()
        .all(|s| *s == TicketStatus::New || *s == TicketStatus::Next);

    if all_complete {
        TicketStatus::Complete
    } else if all_cancelled {
        TicketStatus::Cancelled
    } else if all_finished {
        TicketStatus::Complete
    } else if all_not_started {
        TicketStatus::New
    } else {
        TicketStatus::InProgress
    }
}

/// Get all plans from the plans directory
pub fn get_all_plans() -> Vec<PlanMetadata> {
    let files = find_plans();
    let mut plans = Vec::new();

    for file in files {
        let file_path = PathBuf::from(PLANS_DIR).join(&file);
        match fs::read_to_string(&file_path) {
            Ok(content) => match parse_plan_content(&content) {
                Ok(mut metadata) => {
                    // Ensure ID is set from filename if not in frontmatter
                    if metadata.id.is_none() {
                        metadata.id = Some(file.strip_suffix(".md").unwrap_or(&file).to_string());
                    }
                    metadata.file_path = Some(file_path);
                    plans.push(metadata);
                }
                Err(e) => {
                    eprintln!("Warning: failed to parse plan {}: {}", file, e);
                }
            },
            Err(e) => {
                eprintln!("Warning: failed to read plan {}: {}", file, e);
            }
        }
    }

    plans
}

/// Ensure the plans directory exists
pub fn ensure_plans_dir() -> std::io::Result<()> {
    fs::create_dir_all(PLANS_DIR)
}

/// Generate a unique plan ID with collision checking
pub fn generate_plan_id() -> String {
    use rand::Rng;
    use sha2::{Digest, Sha256};
    use std::path::Path;

    let plans_dir = Path::new(PLANS_DIR);

    loop {
        // Generate random 4-character hex hash
        let random_bytes: [u8; 16] = rand::rng().random();
        let mut hasher = Sha256::new();
        hasher.update(random_bytes);
        let hash = format!("{:x}", hasher.finalize());
        let short_hash = &hash[..4];

        let candidate = format!("plan-{}", short_hash);
        let filename = format!("{}.md", candidate);

        if !plans_dir.join(&filename).exists() {
            return candidate;
        }
        // Collision detected, loop will regenerate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Most parsing tests are in plan_parser.rs.
    // These tests cover plan.rs specific functionality.

    #[test]
    fn test_parse_plan_via_parser() {
        // Test that plan.rs correctly delegates to plan_parser
        let content = r#"---
id: plan-a1b2
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# Test Plan Title

This is the description.
"#;

        let metadata = parse_plan_content(content).unwrap();
        assert_eq!(metadata.id, Some("plan-a1b2".to_string()));
        assert_eq!(
            metadata.uuid,
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
        assert_eq!(metadata.created, Some("2024-01-01T00:00:00Z".to_string()));
        assert_eq!(metadata.title, Some("Test Plan Title".to_string()));
    }

    #[test]
    fn test_compute_aggregate_status_all_complete() {
        let statuses = vec![TicketStatus::Complete, TicketStatus::Complete];
        assert_eq!(compute_aggregate_status(&statuses), TicketStatus::Complete);
    }

    #[test]
    fn test_compute_aggregate_status_all_cancelled() {
        let statuses = vec![TicketStatus::Cancelled, TicketStatus::Cancelled];
        assert_eq!(compute_aggregate_status(&statuses), TicketStatus::Cancelled);
    }

    #[test]
    fn test_compute_aggregate_status_mixed_finished() {
        let statuses = vec![TicketStatus::Complete, TicketStatus::Cancelled];
        assert_eq!(compute_aggregate_status(&statuses), TicketStatus::Complete);
    }

    #[test]
    fn test_compute_aggregate_status_all_not_started() {
        let statuses = vec![TicketStatus::New, TicketStatus::Next];
        assert_eq!(compute_aggregate_status(&statuses), TicketStatus::New);
    }

    #[test]
    fn test_compute_aggregate_status_in_progress() {
        // Some started, some not
        let statuses = vec![TicketStatus::Complete, TicketStatus::New];
        assert_eq!(
            compute_aggregate_status(&statuses),
            TicketStatus::InProgress
        );

        let statuses = vec![TicketStatus::InProgress, TicketStatus::New];
        assert_eq!(
            compute_aggregate_status(&statuses),
            TicketStatus::InProgress
        );

        let statuses = vec![
            TicketStatus::Complete,
            TicketStatus::InProgress,
            TicketStatus::New,
        ];
        assert_eq!(
            compute_aggregate_status(&statuses),
            TicketStatus::InProgress
        );
    }

    #[test]
    fn test_compute_aggregate_status_empty() {
        let statuses: Vec<TicketStatus> = vec![];
        assert_eq!(compute_aggregate_status(&statuses), TicketStatus::New);
    }

    #[test]
    fn test_compute_plan_status_empty_plan() {
        let metadata = PlanMetadata::default();
        let ticket_map = HashMap::new();

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::New);
        assert_eq!(status.completed_count, 0);
        assert_eq!(status.total_count, 0);
    }

    #[test]
    fn test_compute_plan_status_with_tickets() {
        use crate::plan::types::PlanSection;

        let mut metadata = PlanMetadata::default();
        metadata.sections.push(PlanSection::Tickets(vec![
            "j-a1b2".to_string(),
            "j-c3d4".to_string(),
            "j-e5f6".to_string(),
        ]));

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a1b2".to_string(),
            TicketMetadata {
                id: Some("j-a1b2".to_string()),
                status: Some(TicketStatus::Complete),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-c3d4".to_string(),
            TicketMetadata {
                id: Some("j-c3d4".to_string()),
                status: Some(TicketStatus::InProgress),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-e5f6".to_string(),
            TicketMetadata {
                id: Some("j-e5f6".to_string()),
                status: Some(TicketStatus::New),
                ..Default::default()
            },
        );

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::InProgress);
        assert_eq!(status.completed_count, 1);
        assert_eq!(status.total_count, 3);
    }

    #[test]
    fn test_compute_phase_status() {
        let phase = Phase {
            number: "1".to_string(),
            name: "Infrastructure".to_string(),
            description: None,
            success_criteria: vec![],
            tickets: vec!["j-a1b2".to_string(), "j-c3d4".to_string()],
        };

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a1b2".to_string(),
            TicketMetadata {
                id: Some("j-a1b2".to_string()),
                status: Some(TicketStatus::Complete),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-c3d4".to_string(),
            TicketMetadata {
                id: Some("j-c3d4".to_string()),
                status: Some(TicketStatus::Complete),
                ..Default::default()
            },
        );

        let status = compute_phase_status_impl(&phase, &ticket_map);
        assert_eq!(status.phase_number, "1");
        assert_eq!(status.phase_name, "Infrastructure");
        assert_eq!(status.status, TicketStatus::Complete);
        assert_eq!(status.completed_count, 2);
        assert_eq!(status.total_count, 2);
    }

    #[test]
    fn test_compute_phase_status_missing_tickets() {
        let phase = Phase {
            number: "1".to_string(),
            name: "Test".to_string(),
            description: None,
            success_criteria: vec![],
            tickets: vec![
                "j-exists".to_string(),
                "j-missing".to_string(), // Not in ticket_map
            ],
        };

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-exists".to_string(),
            TicketMetadata {
                id: Some("j-exists".to_string()),
                status: Some(TicketStatus::Complete),
                ..Default::default()
            },
        );
        // j-missing is not added

        let status = compute_phase_status_impl(&phase, &ticket_map);
        // Missing tickets are skipped, so we only see the one that exists
        assert_eq!(status.status, TicketStatus::Complete);
        assert_eq!(status.completed_count, 1);
        assert_eq!(status.total_count, 2); // Total still includes missing
    }

    #[test]
    fn test_generate_plan_id_format() {
        let id = generate_plan_id();
        assert!(id.starts_with("plan-"));
        // Format should be plan-XXXX where XXXX is 4 hex chars
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "plan");
        assert_eq!(parts[1].len(), 4);
        // Verify it's hex
        assert!(parts[1].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_plan_with_id() {
        let plan = Plan::with_id("plan-test");
        assert_eq!(plan.id, "plan-test");
        assert_eq!(plan.file_path, PathBuf::from(".janus/plans/plan-test.md"));
    }

    // ============================================================
    // Phased plan status computation tests
    // ============================================================

    /// Helper to create a ticket metadata with a given status
    fn make_ticket(id: &str, status: TicketStatus) -> TicketMetadata {
        TicketMetadata {
            id: Some(id.to_string()),
            status: Some(status),
            ..Default::default()
        }
    }

    /// Helper to create a phased plan with given phase tickets
    fn make_phased_plan(phases: Vec<(&str, &str, Vec<&str>)>) -> PlanMetadata {
        use crate::plan::types::PlanSection;

        let mut metadata = PlanMetadata::default();
        for (number, name, tickets) in phases {
            let phase = Phase {
                number: number.to_string(),
                name: name.to_string(),
                description: None,
                success_criteria: vec![],
                tickets: tickets.iter().map(|s| s.to_string()).collect(),
            };
            metadata.sections.push(PlanSection::Phase(phase));
        }
        metadata
    }

    #[test]
    fn test_compute_phased_plan_status_all_phases_complete() {
        let metadata = make_phased_plan(vec![
            ("1", "Phase One", vec!["t1", "t2"]),
            ("2", "Phase Two", vec!["t3", "t4"]),
        ]);

        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Complete));
        ticket_map.insert("t3".to_string(), make_ticket("t3", TicketStatus::Complete));
        ticket_map.insert("t4".to_string(), make_ticket("t4", TicketStatus::Complete));

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::Complete);
        assert_eq!(status.completed_count, 4);
        assert_eq!(status.total_count, 4);
    }

    #[test]
    fn test_compute_phased_plan_status_mixed_phases() {
        // Phase 1: complete, Phase 2: new
        let metadata = make_phased_plan(vec![
            ("1", "Phase One", vec!["t1", "t2"]),
            ("2", "Phase Two", vec!["t3", "t4"]),
        ]);

        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Complete));
        ticket_map.insert("t3".to_string(), make_ticket("t3", TicketStatus::New));
        ticket_map.insert("t4".to_string(), make_ticket("t4", TicketStatus::New));

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::InProgress);
        assert_eq!(status.completed_count, 2);
        assert_eq!(status.total_count, 4);
    }

    #[test]
    fn test_compute_phased_plan_status_all_new() {
        let metadata = make_phased_plan(vec![
            ("1", "Phase One", vec!["t1"]),
            ("2", "Phase Two", vec!["t2"]),
        ]);

        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::New));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Next));

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::New);
        assert_eq!(status.completed_count, 0);
        assert_eq!(status.total_count, 2);
    }

    #[test]
    fn test_compute_phased_plan_status_all_cancelled() {
        let metadata = make_phased_plan(vec![
            ("1", "Phase One", vec!["t1"]),
            ("2", "Phase Two", vec!["t2"]),
        ]);

        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Cancelled));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Cancelled));

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::Cancelled);
        assert_eq!(status.completed_count, 0);
        assert_eq!(status.total_count, 2);
    }

    #[test]
    fn test_compute_phased_plan_status_mixed_complete_cancelled() {
        // All finished but mixed complete/cancelled should be "complete"
        let metadata = make_phased_plan(vec![("1", "Phase One", vec!["t1", "t2"])]);

        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Cancelled));

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::Complete);
        assert_eq!(status.completed_count, 1); // Only "complete" counts toward completed_count
        assert_eq!(status.total_count, 2);
    }

    #[test]
    fn test_compute_phased_plan_status_in_progress_ticket() {
        let metadata = make_phased_plan(vec![("1", "Phase One", vec!["t1", "t2"])]);

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "t1".to_string(),
            make_ticket("t1", TicketStatus::InProgress),
        );
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::New));

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::InProgress);
        assert_eq!(status.completed_count, 0);
        assert_eq!(status.total_count, 2);
    }

    #[test]
    fn test_compute_phased_plan_status_empty_phases() {
        // Plan with phases but no tickets
        let metadata = make_phased_plan(vec![
            ("1", "Empty Phase", vec![]),
            ("2", "Also Empty", vec![]),
        ]);

        let ticket_map = HashMap::new();

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::New);
        assert_eq!(status.completed_count, 0);
        assert_eq!(status.total_count, 0);
    }

    #[test]
    fn test_compute_all_phase_statuses() {
        let metadata = make_phased_plan(vec![
            ("1", "Phase One", vec!["t1", "t2"]),
            ("2", "Phase Two", vec!["t3"]),
            ("3", "Phase Three", vec!["t4", "t5"]),
        ]);

        let mut ticket_map = HashMap::new();
        // Phase 1: all complete
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Complete));
        // Phase 2: in progress
        ticket_map.insert(
            "t3".to_string(),
            make_ticket("t3", TicketStatus::InProgress),
        );
        // Phase 3: all new
        ticket_map.insert("t4".to_string(), make_ticket("t4", TicketStatus::New));
        ticket_map.insert("t5".to_string(), make_ticket("t5", TicketStatus::New));

        let phase_statuses = compute_all_phase_statuses(&metadata, &ticket_map);

        assert_eq!(phase_statuses.len(), 3);

        // Phase 1
        assert_eq!(phase_statuses[0].phase_number, "1");
        assert_eq!(phase_statuses[0].phase_name, "Phase One");
        assert_eq!(phase_statuses[0].status, TicketStatus::Complete);
        assert_eq!(phase_statuses[0].completed_count, 2);
        assert_eq!(phase_statuses[0].total_count, 2);

        // Phase 2
        assert_eq!(phase_statuses[1].phase_number, "2");
        assert_eq!(phase_statuses[1].phase_name, "Phase Two");
        assert_eq!(phase_statuses[1].status, TicketStatus::InProgress);
        assert_eq!(phase_statuses[1].completed_count, 0);
        assert_eq!(phase_statuses[1].total_count, 1);

        // Phase 3
        assert_eq!(phase_statuses[2].phase_number, "3");
        assert_eq!(phase_statuses[2].phase_name, "Phase Three");
        assert_eq!(phase_statuses[2].status, TicketStatus::New);
        assert_eq!(phase_statuses[2].completed_count, 0);
        assert_eq!(phase_statuses[2].total_count, 2);
    }

    #[test]
    fn test_compute_all_phase_statuses_simple_plan() {
        // Simple plan (no phases) should return empty vec
        use crate::plan::types::PlanSection;

        let mut metadata = PlanMetadata::default();
        metadata.sections.push(PlanSection::Tickets(vec![
            "t1".to_string(),
            "t2".to_string(),
        ]));

        let ticket_map = HashMap::new();
        let phase_statuses = compute_all_phase_statuses(&metadata, &ticket_map);

        assert!(phase_statuses.is_empty());
    }

    #[test]
    fn test_compute_phase_status_empty_phase() {
        let phase = Phase {
            number: "1".to_string(),
            name: "Empty".to_string(),
            description: None,
            success_criteria: vec![],
            tickets: vec![],
        };

        let ticket_map = HashMap::new();
        let status = compute_phase_status_impl(&phase, &ticket_map);

        assert_eq!(status.status, TicketStatus::New);
        assert_eq!(status.completed_count, 0);
        assert_eq!(status.total_count, 0);
    }

    #[test]
    fn test_compute_phased_plan_with_missing_tickets() {
        let metadata = make_phased_plan(vec![("1", "Phase One", vec!["t1", "t2", "t-missing"])]);

        let mut ticket_map = HashMap::new();
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Complete));
        // t-missing is not in ticket_map

        let status = compute_plan_status(&metadata, &ticket_map);
        // Missing tickets are skipped for status computation
        // Both existing tickets are complete, so status should be complete
        assert_eq!(status.status, TicketStatus::Complete);
        assert_eq!(status.completed_count, 2);
        // total_count includes the missing ticket
        assert_eq!(status.total_count, 3);
    }

    #[test]
    fn test_compute_aggregate_status_with_next() {
        // Test that Next status is treated as "not started"
        let statuses = vec![TicketStatus::Next, TicketStatus::New];
        assert_eq!(compute_aggregate_status(&statuses), TicketStatus::New);

        // Next mixed with in_progress should be in_progress
        let statuses = vec![TicketStatus::Next, TicketStatus::InProgress];
        assert_eq!(
            compute_aggregate_status(&statuses),
            TicketStatus::InProgress
        );

        // Next mixed with complete should be in_progress
        let statuses = vec![TicketStatus::Next, TicketStatus::Complete];
        assert_eq!(
            compute_aggregate_status(&statuses),
            TicketStatus::InProgress
        );
    }

    #[test]
    fn test_phase_status_progress_percent() {
        let status = PhaseStatus {
            phase_number: "1".to_string(),
            phase_name: "Test".to_string(),
            status: TicketStatus::InProgress,
            completed_count: 1,
            total_count: 4,
        };

        assert_eq!(status.progress_percent(), 25.0);
        assert_eq!(status.progress_string(), "1/4");
    }

    #[test]
    fn test_plan_status_progress_percent() {
        let status = PlanStatus {
            status: TicketStatus::InProgress,
            completed_count: 3,
            total_count: 10,
        };

        assert_eq!(status.progress_percent(), 30.0);
        assert_eq!(status.progress_string(), "3/10 (30%)");
    }

    #[test]
    fn test_compute_phased_plan_three_phases_progressive() {
        // Realistic scenario: first phase done, second in progress, third not started
        let metadata = make_phased_plan(vec![
            ("1", "Infrastructure", vec!["t1", "t2"]),
            ("2", "Implementation", vec!["t3", "t4", "t5"]),
            ("3", "Testing", vec!["t6", "t7"]),
        ]);

        let mut ticket_map = HashMap::new();
        // Phase 1: all complete
        ticket_map.insert("t1".to_string(), make_ticket("t1", TicketStatus::Complete));
        ticket_map.insert("t2".to_string(), make_ticket("t2", TicketStatus::Complete));
        // Phase 2: in progress (one done, one in progress, one new)
        ticket_map.insert("t3".to_string(), make_ticket("t3", TicketStatus::Complete));
        ticket_map.insert(
            "t4".to_string(),
            make_ticket("t4", TicketStatus::InProgress),
        );
        ticket_map.insert("t5".to_string(), make_ticket("t5", TicketStatus::New));
        // Phase 3: not started
        ticket_map.insert("t6".to_string(), make_ticket("t6", TicketStatus::New));
        ticket_map.insert("t7".to_string(), make_ticket("t7", TicketStatus::New));

        let status = compute_plan_status(&metadata, &ticket_map);
        assert_eq!(status.status, TicketStatus::InProgress);
        assert_eq!(status.completed_count, 3); // t1, t2, t3
        assert_eq!(status.total_count, 7);

        // Verify individual phase statuses
        let phase_statuses = compute_all_phase_statuses(&metadata, &ticket_map);
        assert_eq!(phase_statuses[0].status, TicketStatus::Complete);
        assert_eq!(phase_statuses[1].status, TicketStatus::InProgress);
        assert_eq!(phase_statuses[2].status, TicketStatus::New);
    }
}
