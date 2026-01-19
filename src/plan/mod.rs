pub mod parser;
pub mod status;
pub mod types;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::cache;
use crate::error::{JanusError, Result};
use crate::hooks::{HookContext, HookEvent, ItemType, run_post_hooks, run_pre_hooks};
use crate::plan::parser::parse_plan_content;
use crate::plan::types::{Phase, PhaseStatus, PlanMetadata, PlanStatus};
use crate::types::{PLANS_DIR, TicketMetadata};

// Re-export status computation functions
pub use crate::plan::status::{
    compute_aggregate_status, compute_all_phase_statuses, compute_phase_status,
    compute_plan_status, resolve_ticket_or_warn,
};

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
pub async fn find_plan_by_id(partial_id: &str) -> Result<PathBuf> {
    // Try cache first
    if let Some(cache) = cache::get_or_init_cache().await {
        // Exact match check - file exists?
        let exact_match_path = PathBuf::from(PLANS_DIR).join(format!("{}.md", partial_id));
        if exact_match_path.exists() {
            return Ok(exact_match_path);
        }

        // Partial match via cache
        if let Ok(matches) = cache.find_plan_by_partial_id(partial_id).await {
            match matches.len() {
                0 => {}
                1 => {
                    let filename = format!("{}.md", &matches[0]);
                    return Ok(PathBuf::from(PLANS_DIR).join(filename));
                }
                _ => {
                    return Err(JanusError::AmbiguousPlanId(
                        partial_id.to_string(),
                        matches.clone(),
                    ));
                }
            }
        }
    }

    // FALLBACK: Original file-based implementation
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
        _ => Err(JanusError::AmbiguousPlanId(
            partial_id.to_string(),
            matches.iter().map(|m| m.replace(".md", "")).collect(),
        )),
    }
}

/// A plan handle for reading and writing plan files
pub struct Plan {
    pub file_path: PathBuf,
    pub id: String,
}

impl Plan {
    /// Find a plan by its (partial) ID
    pub async fn find(partial_id: &str) -> Result<Self> {
        let file_path = find_plan_by_id(partial_id).await?;
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
    ///
    /// This method triggers `PreWrite` hook before writing, and `PostWrite` + `PlanUpdated`
    /// hooks after successful write.
    pub fn write(&self, content: &str) -> Result<()> {
        // Build hook context
        let context = HookContext::new()
            .with_item_type(ItemType::Plan)
            .with_item_id(&self.id)
            .with_file_path(&self.file_path);

        // Run pre-write hook (can abort)
        run_pre_hooks(HookEvent::PreWrite, &context)?;

        // Perform the write
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.file_path, content)?;

        // Run post-write hooks (fire-and-forget)
        run_post_hooks(HookEvent::PostWrite, &context);
        run_post_hooks(HookEvent::PlanUpdated, &context);

        Ok(())
    }

    /// Write content to the plan file without triggering hooks.
    ///
    /// Used internally when hooks should be handled at a higher level
    /// (e.g., plan creation where PlanCreated should be fired instead of PlanUpdated).
    pub(crate) fn write_without_hooks(&self, content: &str) -> Result<()> {
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.file_path, content)?;
        Ok(())
    }

    /// Delete the plan file
    ///
    /// This method triggers `PreDelete` hook before deletion, and `PostDelete` + `PlanDeleted`
    /// hooks after successful deletion.
    pub fn delete(&self) -> Result<()> {
        if !self.file_path.exists() {
            return Ok(());
        }

        // Build hook context
        let context = HookContext::new()
            .with_item_type(ItemType::Plan)
            .with_item_id(&self.id)
            .with_file_path(&self.file_path);

        // Run pre-delete hook (can abort)
        run_pre_hooks(HookEvent::PreDelete, &context)?;

        // Perform the delete
        fs::remove_file(&self.file_path)?;

        // Run post-delete hooks (fire-and-forget)
        run_post_hooks(HookEvent::PostDelete, &context);
        run_post_hooks(HookEvent::PlanDeleted, &context);

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
        compute_phase_status(phase, ticket_map)
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

/// Get all plans from the plans directory
pub async fn get_all_plans() -> Vec<PlanMetadata> {
    // Try cache first
    if let Some(cache) = cache::get_or_init_cache().await {
        if let Ok(cached_plans) = cache.get_all_plans().await {
            // Convert cached plans to full PlanMetadata
            let mut plans = Vec::new();
            for cached in cached_plans {
                if let Some(id) = &cached.id {
                    let file_path = PathBuf::from(PLANS_DIR).join(format!("{}.md", id));
                    // If the file exists, read it to get full metadata
                    // This ensures we get all sections, not just cached fields
                    if file_path.exists()
                        && let Ok(content) = fs::read_to_string(&file_path)
                        && let Ok(mut metadata) = parse_plan_content(&content)
                    {
                        if metadata.id.is_none() {
                            metadata.id = Some(id.clone());
                        }
                        metadata.file_path = Some(file_path);
                        plans.push(metadata);
                    }
                }
            }
            // Add plans that exist on disk but maybe not in cache (shouldn't happen after sync)
            if plans.is_empty() {
                eprintln!("Warning: cache read failed, falling back to file reads");
                return get_all_plans_from_disk();
            }
            return plans;
        }
        eprintln!("Warning: cache read failed, falling back to file reads");
    }

    // FALLBACK: Original implementation
    get_all_plans_from_disk()
}

/// Get all plans from disk (fallback implementation)
pub fn get_all_plans_from_disk() -> Vec<PlanMetadata> {
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
    use std::path::Path;

    use crate::utils::generate_hash;

    const RETRIES_PER_LENGTH: u32 = 40;
    let plans_dir = Path::new(PLANS_DIR);

    for length in 4..=8 {
        for _ in 0..RETRIES_PER_LENGTH {
            let hash = generate_hash(length);
            let candidate = format!("plan-{}", hash);
            let filename = format!("{}.md", candidate);

            if !plans_dir.join(&filename).exists() {
                return candidate;
            }
        }
    }

    panic!(
        "Failed to generate unique plan ID after trying hash lengths 4-8 with {} retries each",
        RETRIES_PER_LENGTH
    );
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
}
