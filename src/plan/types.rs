use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::types::TicketStatus;

pub struct Progress {
    pub completed: usize,
    pub total: usize,
}

impl Progress {
    pub fn percent(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.completed as f64 / self.total as f64) * 100.0
        }
    }

    pub fn format(&self) -> String {
        format!("{}/{} ({:.0}%)", self.completed, self.total, self.percent())
    }
}

/// Metadata parsed from a plan file's YAML frontmatter and markdown body
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlanMetadata {
    /// Plan ID (e.g., "plan-a1b2")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Durable UUID v4 for disambiguation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,

    /// Plan title extracted from H1 heading
    #[serde(skip)]
    pub title: Option<String>,

    /// Creation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,

    /// Description: content between title (H1) and first H2 section
    #[serde(skip)]
    pub description: Option<String>,

    /// Acceptance criteria extracted from `## Acceptance Criteria` section
    #[serde(skip)]
    pub acceptance_criteria: Vec<String>,

    /// Ordered list of all sections (phases, tickets, free-form)
    #[serde(skip)]
    pub sections: Vec<PlanSection>,

    /// Path to the plan file on disk
    #[serde(skip)]
    pub file_path: Option<PathBuf>,
}

impl PlanMetadata {
    /// Get all phases (filters sections to Phase variants)
    pub fn phases(&self) -> Vec<&Phase> {
        self.sections
            .iter()
            .filter_map(|s| match s {
                PlanSection::Phase(phase) => Some(phase),
                _ => None,
            })
            .collect()
    }

    /// Get all phases mutably
    pub fn phases_mut(&mut self) -> Vec<&mut Phase> {
        self.sections
            .iter_mut()
            .filter_map(|s| match s {
                PlanSection::Phase(phase) => Some(phase),
                _ => None,
            })
            .collect()
    }

    /// Get all tickets across all phases (or from Tickets section for simple plans)
    pub fn all_tickets(&self) -> Vec<&str> {
        let mut tickets = Vec::new();
        for section in &self.sections {
            match section {
                PlanSection::Phase(phase) => {
                    tickets.extend(phase.tickets.iter().map(|s| s.as_str()));
                }
                PlanSection::Tickets(ticket_list) => {
                    tickets.extend(ticket_list.iter().map(|s| s.as_str()));
                }
                PlanSection::FreeForm(_) => {}
            }
        }
        tickets
    }

    /// Check if this is a phased plan (has at least one Phase section)
    pub fn is_phased(&self) -> bool {
        self.sections
            .iter()
            .any(|s| matches!(s, PlanSection::Phase(_)))
    }

    /// Check if this is a simple plan (has a Tickets section, no phases)
    pub fn is_simple(&self) -> bool {
        self.sections
            .iter()
            .any(|s| matches!(s, PlanSection::Tickets(_)))
            && !self.is_phased()
    }

    /// Get all free-form sections
    pub fn free_form_sections(&self) -> Vec<&FreeFormSection> {
        self.sections
            .iter()
            .filter_map(|s| match s {
                PlanSection::FreeForm(ff) => Some(ff),
                _ => None,
            })
            .collect()
    }

    /// Get the tickets section for simple plans (returns None for phased plans)
    pub fn tickets_section(&self) -> Option<&Vec<String>> {
        self.sections.iter().find_map(|s| match s {
            PlanSection::Tickets(tickets) => Some(tickets),
            _ => None,
        })
    }

    /// Get the tickets section mutably for simple plans
    pub fn tickets_section_mut(&mut self) -> Option<&mut Vec<String>> {
        self.sections.iter_mut().find_map(|s| match s {
            PlanSection::Tickets(tickets) => Some(tickets),
            _ => None,
        })
    }

    /// Find a phase by number (e.g., "1", "2a")
    pub fn find_phase_by_number(&self, number: &str) -> Option<&Phase> {
        self.phases()
            .into_iter()
            .find(|p| p.number.eq_ignore_ascii_case(number))
    }

    /// Find a phase by name (case-insensitive)
    pub fn find_phase_by_name(&self, name: &str) -> Option<&Phase> {
        self.phases()
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Find a phase by number or name
    pub fn find_phase(&self, identifier: &str) -> Option<&Phase> {
        self.find_phase_by_number(identifier)
            .or_else(|| self.find_phase_by_name(identifier))
    }

    /// Find a phase mutably by number or name
    pub fn find_phase_mut(&mut self, identifier: &str) -> Option<&mut Phase> {
        let identifier_lower = identifier.to_lowercase();
        self.sections.iter_mut().find_map(|s| match s {
            PlanSection::Phase(phase) => {
                if phase.number.eq_ignore_ascii_case(&identifier_lower)
                    || phase.name.to_lowercase() == identifier_lower
                {
                    Some(phase)
                } else {
                    None
                }
            }
            _ => None,
        })
    }
}

impl crate::types::ItemMetadata for PlanMetadata {
    fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn uuid(&self) -> Option<&str> {
        self.uuid.as_deref()
    }

    fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    fn file_path(&self) -> Option<&PathBuf> {
        self.file_path.as_ref()
    }

    fn item_type(&self) -> crate::types::EntityType {
        crate::types::EntityType::Plan
    }
}

/// A section in a plan - either structured (phase/tickets) or free-form
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlanSection {
    /// Structured phase with tickets
    Phase(Phase),
    /// Structured ticket list (for simple plans without phases)
    Tickets(Vec<String>),
    /// Free-form content preserved verbatim
    FreeForm(FreeFormSection),
}

/// A phase within a phased plan
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Phase {
    /// Phase number/identifier (e.g., "1", "2a", "2b", "10")
    pub number: String,

    /// Phase name (e.g., "Infrastructure", "Sync Algorithm")
    pub name: String,

    /// Phase description (content after header, before Success Criteria/Tickets)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Success criteria for this phase
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub success_criteria: Vec<String>,

    /// Ordered list of ticket IDs in this phase
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tickets: Vec<String>,
}

impl Phase {
    /// Create a new phase with the given number and name
    pub fn new(number: impl Into<String>, name: impl Into<String>) -> Self {
        Phase {
            number: number.into(),
            name: name.into(),
            description: None,
            success_criteria: Vec::new(),
            tickets: Vec::new(),
        }
    }

    /// Get the phase number
    pub fn number(&self) -> &str {
        &self.number
    }

    /// Get the phase name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the tickets in this phase
    pub fn tickets(&self) -> &[String] {
        &self.tickets
    }

    /// Check if a ticket ID is in this phase
    pub fn contains_ticket(&self, ticket_id: &str) -> bool {
        self.tickets.iter().any(|t| t == ticket_id)
    }

    /// Add a ticket to this phase at the end
    pub fn add_ticket(&mut self, ticket_id: impl Into<String>) {
        self.tickets.push(ticket_id.into());
    }

    /// Add a ticket at a specific position (1-indexed)
    pub fn add_ticket_at_position(&mut self, ticket_id: impl Into<String>, position: usize) {
        let ticket = ticket_id.into();
        // Position is 1-indexed, convert to 0-indexed
        let index = position.saturating_sub(1);
        if index >= self.tickets.len() {
            self.tickets.push(ticket);
        } else {
            self.tickets.insert(index, ticket);
        }
    }

    /// Add a ticket after another ticket
    pub fn add_ticket_after(&mut self, ticket_id: impl Into<String>, after_ticket: &str) -> bool {
        if let Some(pos) = self.tickets.iter().position(|t| t == after_ticket) {
            self.tickets.insert(pos + 1, ticket_id.into());
            true
        } else {
            false
        }
    }

    /// Remove a ticket from this phase
    pub fn remove_ticket(&mut self, ticket_id: &str) -> bool {
        if let Some(pos) = self.tickets.iter().position(|t| t == ticket_id) {
            self.tickets.remove(pos);
            true
        } else {
            false
        }
    }
}

/// Free-form section preserved verbatim
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FreeFormSection {
    /// The H2 heading text (without the `## ` prefix)
    pub heading: String,

    /// Full markdown content after the heading (preserved verbatim)
    pub content: String,
}

impl FreeFormSection {
    /// Create a new free-form section
    pub fn new(heading: impl Into<String>, content: impl Into<String>) -> Self {
        FreeFormSection {
            heading: heading.into(),
            content: content.into(),
        }
    }
}

/// Computed status for an entire plan
#[derive(Debug, Clone)]
pub struct PlanStatus {
    /// Computed status based on constituent tickets
    pub status: TicketStatus,

    /// Number of completed tickets
    pub completed_count: usize,

    /// Total number of tickets
    pub total_count: usize,
}

impl PlanStatus {
    /// Number of completed tickets
    pub fn completed_count(&self) -> usize {
        self.completed_count
    }

    /// Total number of tickets
    pub fn total_count(&self) -> usize {
        self.total_count
    }

    /// Get progress information
    pub fn progress(&self) -> Progress {
        Progress {
            completed: self.completed_count,
            total: self.total_count,
        }
    }

    /// Get progress as a percentage (0.0 to 100.0)
    pub fn progress_percent(&self) -> f64 {
        self.progress().percent()
    }

    /// Check if all tickets are complete
    pub fn is_complete(&self) -> bool {
        self.total_count > 0 && self.completed_count == self.total_count
    }

    /// Check if there are no tickets
    pub fn is_empty(&self) -> bool {
        self.total_count == 0
    }

    /// Format progress as a string (e.g., "5/12 (41%)")
    pub fn progress_string(&self) -> String {
        self.progress().format()
    }
}

impl Default for PlanStatus {
    fn default() -> Self {
        PlanStatus {
            status: TicketStatus::New,
            completed_count: 0,
            total_count: 0,
        }
    }
}

/// Computed status for a single phase
#[derive(Debug, Clone)]
pub struct PhaseStatus {
    /// Phase number (e.g., "1", "2a")
    pub phase_number: String,

    /// Phase name
    pub phase_name: String,

    /// Computed status based on phase's tickets
    pub status: TicketStatus,

    /// Number of completed tickets in this phase
    pub completed_count: usize,

    /// Total number of tickets in this phase
    pub total_count: usize,
}

impl PhaseStatus {
    /// Number of completed tickets in this phase
    pub fn completed_count(&self) -> usize {
        self.completed_count
    }

    /// Total number of tickets in this phase
    pub fn total_count(&self) -> usize {
        self.total_count
    }

    /// Get progress information
    pub fn progress(&self) -> Progress {
        Progress {
            completed: self.completed_count,
            total: self.total_count,
        }
    }

    /// Get progress as a percentage (0.0 to 100.0)
    pub fn progress_percent(&self) -> f64 {
        self.progress().percent()
    }

    /// Check if all tickets in this phase are complete
    pub fn is_complete(&self) -> bool {
        self.total_count > 0 && self.completed_count == self.total_count
    }

    /// Check if there are no tickets in this phase
    pub fn is_empty(&self) -> bool {
        self.total_count == 0
    }

    /// Format progress as a string (e.g., "2/4")
    ///
    /// Note: Unlike PlanStatus, this does not include the percentage in the output.
    /// If percentage is needed, use `progress_percent()`.
    pub fn progress_string(&self) -> String {
        format!("{}/{}", self.completed_count, self.total_count)
    }
}

// ============================================================
// Importable Plan Types (for plan import functionality)
// ============================================================

/// Represents a parsed importable plan before ticket creation.
///
/// This is the intermediate representation produced by parsing an AI-generated
/// plan document. It contains all the information needed to create tickets
/// and a plan, but has not yet been persisted to disk.
///
/// The expected document format is:
/// - `# Title` (H1) - Required plan title
/// - Description paragraphs after title
/// - `## Design` - Required design section
/// - `## Acceptance Criteria` - Optional, creates verification ticket
/// - `## Implementation` - Required, contains all phases
///   - `### Phase N: Name` (H3) - Phases under Implementation
///     - `#### Task Title` (H4) - Tasks under each phase
#[derive(Debug, Clone, Default)]
pub struct ImportablePlan {
    /// Plan title (extracted from H1 heading)
    pub title: String,

    /// Optional description (content between H1 and first H2)
    pub description: Option<String>,

    /// Design section content (from `## Design` section) - required
    pub design: Option<String>,

    /// Acceptance criteria (from `## Acceptance Criteria` section)
    pub acceptance_criteria: Vec<String>,

    /// Phases containing tasks (under `## Implementation` section)
    pub phases: Vec<ImportablePhase>,
}

impl ImportablePlan {
    /// Check if this plan has phases (it always does in the new format)
    pub fn is_phased(&self) -> bool {
        !self.phases.is_empty()
    }

    /// Get the total number of tasks across all phases
    pub fn task_count(&self) -> usize {
        self.phases.iter().map(|p| p.tasks.len()).sum()
    }

    /// Get all tasks from all phases
    pub fn all_tasks(&self) -> Vec<&ImportableTask> {
        self.phases.iter().flat_map(|p| p.tasks.iter()).collect()
    }
}

/// Represents a phase within an importable plan.
///
/// A phase groups related tasks together and typically represents
/// a milestone or stage in the implementation.
#[derive(Debug, Clone, Default)]
pub struct ImportablePhase {
    /// Phase number/identifier (e.g., "1", "2a", "3")
    pub number: String,

    /// Phase name (e.g., "Infrastructure", "Core Implementation")
    pub name: String,

    /// Optional phase description (content after header, before tasks)
    pub description: Option<String>,

    /// Tasks within this phase
    pub tasks: Vec<ImportableTask>,
}

impl ImportablePhase {
    /// Get the phase number
    pub fn number(&self) -> &str {
        &self.number
    }

    /// Get the phase name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the tasks in this phase
    pub fn tasks(&self) -> &[ImportableTask] {
        &self.tasks
    }
}

/// Represents a task within an importable plan.
///
/// Each task will become a ticket when the plan is imported.
#[derive(Debug, Clone, Default)]
pub struct ImportableTask {
    /// Task title (becomes ticket title)
    pub title: String,

    /// Task body/description (becomes ticket body)
    pub body: Option<String>,

    /// Whether the task is marked as complete (e.g., `[x]` marker)
    pub is_complete: bool,
}

/// Detailed validation error for plan import.
///
pub type ImportValidationError = (Option<usize>, String, Option<String>);

pub fn display_import_validation_error(error: &ImportValidationError) -> String {
    let mut result = String::new();

    if let Some(line) = error.0 {
        result.push_str(&format!("Line {}: ", line));
    }

    result.push_str(&error.1);

    if let Some(hint) = &error.2 {
        result.push_str(&format!("\n    Hint: {}", hint));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_metadata_default() {
        let plan = PlanMetadata::default();
        assert!(plan.id.is_none());
        assert!(plan.title.is_none());
        assert!(plan.sections.is_empty());
        assert!(!plan.is_phased());
        assert!(!plan.is_simple());
    }

    #[test]
    fn test_plan_metadata_is_phased() {
        let mut plan = PlanMetadata::default();
        plan.sections
            .push(PlanSection::Phase(Phase::new("1", "Infrastructure")));

        assert!(plan.is_phased());
        assert!(!plan.is_simple());
    }

    #[test]
    fn test_plan_metadata_is_simple() {
        let mut plan = PlanMetadata::default();
        plan.sections
            .push(PlanSection::Tickets(vec!["j-a1b2".to_string()]));

        assert!(!plan.is_phased());
        assert!(plan.is_simple());
    }

    #[test]
    fn test_plan_metadata_all_tickets_simple() {
        let mut plan = PlanMetadata::default();
        plan.sections.push(PlanSection::Tickets(vec![
            "j-a1b2".to_string(),
            "j-c3d4".to_string(),
        ]));

        let tickets = plan.all_tickets();
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4"]);
    }

    #[test]
    fn test_plan_metadata_all_tickets_phased() {
        let mut plan = PlanMetadata::default();

        let mut phase1 = Phase::new("1", "Phase One");
        phase1.tickets = vec!["j-a1b2".to_string(), "j-c3d4".to_string()];

        let mut phase2 = Phase::new("2", "Phase Two");
        phase2.tickets = vec!["j-e5f6".to_string()];

        plan.sections.push(PlanSection::Phase(phase1));
        plan.sections
            .push(PlanSection::FreeForm(FreeFormSection::new(
                "Notes",
                "Some notes",
            )));
        plan.sections.push(PlanSection::Phase(phase2));

        let tickets = plan.all_tickets();
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);
    }

    #[test]
    fn test_plan_metadata_phases() {
        let mut plan = PlanMetadata::default();
        plan.sections
            .push(PlanSection::Phase(Phase::new("1", "First")));
        plan.sections
            .push(PlanSection::FreeForm(FreeFormSection::new("Notes", "")));
        plan.sections
            .push(PlanSection::Phase(Phase::new("2", "Second")));

        let phases = plan.phases();
        assert_eq!(phases.len(), 2);
        assert_eq!(phases[0].name, "First");
        assert_eq!(phases[1].name, "Second");
    }

    #[test]
    fn test_plan_metadata_find_phase() {
        let mut plan = PlanMetadata::default();
        plan.sections
            .push(PlanSection::Phase(Phase::new("1", "Infrastructure")));
        plan.sections
            .push(PlanSection::Phase(Phase::new("2a", "Sync Part A")));

        // Find by number
        assert!(plan.find_phase("1").is_some());
        assert_eq!(plan.find_phase("1").unwrap().name, "Infrastructure");

        // Find by name (case-insensitive)
        assert!(plan.find_phase("infrastructure").is_some());
        assert!(plan.find_phase("INFRASTRUCTURE").is_some());

        // Find by number with letter
        assert!(plan.find_phase("2a").is_some());
        assert_eq!(plan.find_phase("2a").unwrap().name, "Sync Part A");

        // Not found
        assert!(plan.find_phase("3").is_none());
        assert!(plan.find_phase("nonexistent").is_none());
    }

    #[test]
    fn test_plan_metadata_free_form_sections() {
        let mut plan = PlanMetadata::default();
        plan.sections
            .push(PlanSection::FreeForm(FreeFormSection::new(
                "Overview", "Content",
            )));
        plan.sections
            .push(PlanSection::Phase(Phase::new("1", "First")));
        plan.sections
            .push(PlanSection::FreeForm(FreeFormSection::new(
                "Notes",
                "More content",
            )));

        let ff = plan.free_form_sections();
        assert_eq!(ff.len(), 2);
        assert_eq!(ff[0].heading, "Overview");
        assert_eq!(ff[1].heading, "Notes");
    }

    #[test]
    fn test_phase_new() {
        let phase = Phase::new("1", "Infrastructure");
        assert_eq!(phase.number, "1");
        assert_eq!(phase.name, "Infrastructure");
        assert!(phase.description.is_none());
        assert!(phase.success_criteria.is_empty());
        assert!(phase.tickets.is_empty());
    }

    #[test]
    fn test_phase_ticket_operations() {
        let mut phase = Phase::new("1", "Test Phase");

        // Add tickets
        phase.add_ticket("j-a1b2");
        phase.add_ticket("j-c3d4");
        assert_eq!(phase.tickets, vec!["j-a1b2", "j-c3d4"]);

        // Check contains
        assert!(phase.contains_ticket("j-a1b2"));
        assert!(!phase.contains_ticket("j-e5f6"));

        // Add at position (1-indexed)
        phase.add_ticket_at_position("j-e5f6", 2);
        assert_eq!(phase.tickets, vec!["j-a1b2", "j-e5f6", "j-c3d4"]);

        // Add after
        phase.add_ticket_after("j-g7h8", "j-e5f6");
        assert_eq!(phase.tickets, vec!["j-a1b2", "j-e5f6", "j-g7h8", "j-c3d4"]);

        // Remove
        assert!(phase.remove_ticket("j-e5f6"));
        assert_eq!(phase.tickets, vec!["j-a1b2", "j-g7h8", "j-c3d4"]);
        assert!(!phase.remove_ticket("nonexistent"));
    }

    #[test]
    fn test_phase_add_ticket_at_position_edge_cases() {
        let mut phase = Phase::new("1", "Test");
        phase.tickets = vec!["t1".to_string(), "t2".to_string()];

        // Position 1 (first)
        phase.add_ticket_at_position("t0", 1);
        assert_eq!(phase.tickets, vec!["t0", "t1", "t2"]);

        // Position beyond length (append)
        phase.add_ticket_at_position("t3", 100);
        assert_eq!(phase.tickets, vec!["t0", "t1", "t2", "t3"]);

        // Position 0 (should be same as 1 due to saturating_sub)
        phase.add_ticket_at_position("t-1", 0);
        assert_eq!(phase.tickets, vec!["t-1", "t0", "t1", "t2", "t3"]);
    }

    #[test]
    fn test_free_form_section_new() {
        let section = FreeFormSection::new("SQLite Schema", "```sql\nCREATE TABLE...\n```");
        assert_eq!(section.heading, "SQLite Schema");
        assert!(section.content.contains("CREATE TABLE"));
    }

    #[test]
    fn test_plan_status_progress() {
        let status = PlanStatus {
            status: TicketStatus::InProgress,
            completed_count: 5,
            total_count: 12,
        };

        let progress = Progress {
            completed: 5,
            total: 12,
        };

        assert!((status.progress_percent() - progress.percent()).abs() < 0.01);
        assert_eq!(status.progress_string(), progress.format());
    }

    #[test]
    fn test_plan_status_empty() {
        let status = PlanStatus::default();
        let progress = Progress {
            completed: 0,
            total: 0,
        };
        assert_eq!(status.progress_percent(), progress.percent());
        assert_eq!(status.progress_string(), progress.format());
    }

    #[test]
    fn test_phase_status_progress() {
        let status = PhaseStatus {
            phase_number: "1".to_string(),
            phase_name: "Infrastructure".to_string(),
            status: TicketStatus::Complete,
            completed_count: 2,
            total_count: 2,
        };

        let progress = Progress {
            completed: 2,
            total: 2,
        };

        assert_eq!(status.progress_percent(), progress.percent());
        assert_eq!(status.progress_string(), "2/2");
    }

    #[test]
    fn test_plan_metadata_tickets_section_mut() {
        let mut plan = PlanMetadata::default();
        plan.sections.push(PlanSection::Tickets(vec![
            "j-a1b2".to_string(),
            "j-c3d4".to_string(),
        ]));

        // Get mutable reference and modify
        if let Some(tickets) = plan.tickets_section_mut() {
            tickets.push("j-e5f6".to_string());
        }

        let tickets = plan.all_tickets();
        assert_eq!(tickets.len(), 3);
        assert!(tickets.contains(&"j-e5f6"));
    }

    #[test]
    fn test_plan_metadata_find_phase_mut() {
        let mut plan = PlanMetadata::default();
        let mut phase = Phase::new("1", "Infrastructure");
        phase.tickets = vec!["j-a1b2".to_string()];
        plan.sections.push(PlanSection::Phase(phase));

        // Find and modify
        if let Some(p) = plan.find_phase_mut("1") {
            p.tickets.push("j-c3d4".to_string());
        }

        let tickets = plan.all_tickets();
        assert_eq!(tickets.len(), 2);
        assert!(tickets.contains(&"j-c3d4"));
    }

    // ============================================================
    // Importable Plan Types Tests
    // ============================================================

    #[test]
    fn test_importable_plan_default() {
        let plan = ImportablePlan::default();
        assert!(plan.title.is_empty());
        assert!(plan.description.is_none());
        assert!(plan.acceptance_criteria.is_empty());
        assert!(plan.phases.is_empty());
        assert!(plan.design.is_none());
        assert_eq!(plan.task_count(), 0);
    }

    #[test]
    fn test_importable_plan_with_phases() {
        let plan = ImportablePlan {
            title: "Test Plan".to_string(),
            description: Some("A test plan".to_string()),
            design: Some("Design details".to_string()),
            acceptance_criteria: vec!["Criterion 1".to_string()],
            phases: vec![
                ImportablePhase {
                    number: "1".to_string(),
                    name: "Phase One".to_string(),
                    description: None,
                    tasks: vec![
                        ImportableTask {
                            title: "Task 1".to_string(),
                            body: Some("Body 1".to_string()),
                            is_complete: false,
                        },
                        ImportableTask {
                            title: "Task 2".to_string(),
                            body: None,
                            is_complete: true,
                        },
                    ],
                },
                ImportablePhase {
                    number: "2".to_string(),
                    name: "Phase Two".to_string(),
                    description: Some("Second phase".to_string()),
                    tasks: vec![ImportableTask {
                        title: "Task 3".to_string(),
                        body: None,
                        is_complete: false,
                    }],
                },
            ],
        };

        assert_eq!(plan.task_count(), 3);
        assert_eq!(plan.all_tasks().len(), 3);
        assert_eq!(plan.all_tasks()[0].title, "Task 1");
        assert_eq!(plan.all_tasks()[1].title, "Task 2");
        assert_eq!(plan.all_tasks()[2].title, "Task 3");
    }

    #[test]
    fn test_importable_task_default() {
        let task = ImportableTask::default();
        assert!(task.title.is_empty());
        assert!(task.body.is_none());
        assert!(!task.is_complete);
    }

    #[test]
    fn test_importable_phase_default() {
        let phase = ImportablePhase::default();
        assert!(phase.number.is_empty());
        assert!(phase.name.is_empty());
        assert!(phase.description.is_none());
        assert!(phase.tasks.is_empty());
    }

    #[test]
    fn test_import_validation_error_new() {
        let err = (None, "Missing title".to_string(), None);
        assert!(err.0.is_none());
        assert_eq!(err.1, "Missing title");
        assert!(err.2.is_none());
        assert_eq!(display_import_validation_error(&err), "Missing title");
    }

    #[test]
    fn test_import_validation_error_at_line() {
        let err = (Some(42), "Invalid syntax".to_string(), None);
        assert_eq!(err.0, Some(42));
        assert_eq!(err.1, "Invalid syntax");
        assert!(err.2.is_none());
        assert_eq!(
            display_import_validation_error(&err),
            "Line 42: Invalid syntax"
        );
    }

    #[test]
    fn test_import_validation_error_with_hint() {
        let err = (
            None,
            "Missing plan title".to_string(),
            Some("Add a # Title heading".to_string()),
        );
        assert!(err.0.is_none());
        assert_eq!(err.1, "Missing plan title");
        assert_eq!(err.2, Some("Add a # Title heading".to_string()));
        assert_eq!(
            display_import_validation_error(&err),
            "Missing plan title\n    Hint: Add a # Title heading"
        );
    }

    #[test]
    fn test_import_validation_error_full() {
        let err = (
            Some(10),
            "Phase has no tasks".to_string(),
            Some("Add ### Task headers under the phase".to_string()),
        );
        assert_eq!(err.0, Some(10));
        assert_eq!(err.1, "Phase has no tasks");
        assert_eq!(
            err.2,
            Some("Add ### Task headers under the phase".to_string())
        );
        assert_eq!(
            display_import_validation_error(&err),
            "Line 10: Phase has no tasks\n    Hint: Add ### Task headers under the phase"
        );
    }

    #[test]
    fn test_import_validation_error_display_fn() {
        let err = (Some(5), "Test error".to_string(), None);
        assert_eq!(display_import_validation_error(&err), "Line 5: Test error");
    }

    #[test]
    fn test_progress() {
        let progress = Progress {
            completed: 5,
            total: 12,
        };

        assert_eq!(progress.percent(), 5.0 / 12.0 * 100.0);
        assert_eq!(progress.format(), "5/12 (42%)");
    }

    #[test]
    fn test_progress_empty() {
        let progress = Progress {
            completed: 0,
            total: 0,
        };

        assert_eq!(progress.percent(), 0.0);
        assert_eq!(progress.format(), "0/0 (0%)");
    }

    // ============================================================
    // Phase Identity Trait Tests
    // ============================================================

    #[test]
    fn test_phase_has_phase_identity() {
        let phase = Phase::new("1", "Infrastructure");
        assert_eq!(phase.number(), "1");
        assert_eq!(phase.name(), "Infrastructure");
    }

    #[test]
    fn test_phase_has_phase_content() {
        let mut phase = Phase::new("1", "Test");
        phase.add_ticket("j-a1b2");
        phase.add_ticket("j-c3d4");

        let tickets = phase.tickets();
        assert_eq!(tickets, &["j-a1b2", "j-c3d4"]);
    }

    #[test]
    fn test_importable_phase_inherent_methods() {
        let phase = ImportablePhase {
            number: "2a".to_string(),
            name: "Sync Part A".to_string(),
            description: None,
            tasks: vec![],
        };

        assert_eq!(phase.number(), "2a");
        assert_eq!(phase.name(), "Sync Part A");
    }
}
