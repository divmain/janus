//! Plan module with simplified direct file operations
//!
//! This module provides the `Plan` type which handles file I/O directly,
//! similar to the `Ticket` type. Plans are stored as Markdown files with
//! YAML frontmatter in the `.janus/plans/` directory.

pub mod parser;
pub mod types;

pub use types::{
    ImportValidationError, ImportablePhase, ImportablePlan, ImportableTask, Phase, PhaseStatus,
    PlanLoadResult, PlanMetadata, PlanSection, PlanStatus,
};

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::cache::get_or_init_store;
use crate::entity::Entity;
use crate::error::{FileOperation, JanusError, Result, format_file_error};
use crate::hooks::{HookContext, HookEvent, run_post_hooks, run_pre_hooks};
use crate::plan::parser::{parse_plan_content, serialize_plan};
use crate::types::{EntityType, TicketMetadata, plans_dir};
use crate::utils::{DirScanner, extract_id_from_path};

// Re-export status computation functions
pub use crate::status::plan::{
    compute_aggregate_status, compute_all_phase_statuses, compute_phase_status,
    compute_plan_status, resolve_ticket_or_warn,
};

// Re-export parser functions for plan import
pub use crate::plan::parser::{
    ACCEPTANCE_CRITERIA_ALIASES, DESIGN_SECTION_NAME, IMPLEMENTATION_SECTION_NAME, PHASE_PATTERN,
    is_completed_task, is_phase_header, is_section_alias, parse_importable_plan,
};

/// Find all plan files in the plans directory
fn find_plans() -> Vec<String> {
    DirScanner::find_markdown_files(plans_dir()).unwrap_or_else(|e| {
        eprintln!("Warning: failed to read plans directory: {e}");
        Vec::new()
    })
}

/// Entity type for ID validation error messages.
#[derive(Debug, Clone, Copy)]
enum EntityKind {
    Plan,
}

/// Validate that an ID is safe for filesystem use (no path traversal)
fn validate_id(id: &str, kind: EntityKind) -> Result<()> {
    let make_error = |id: &str| match kind {
        EntityKind::Plan => JanusError::InvalidPlanId(id.to_string()),
    };

    // Check for path separators and parent directory references
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(make_error(id));
    }

    // Ensure ID contains only alphanumeric characters, hyphens, and underscores
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(make_error(id));
    }

    Ok(())
}

/// Find a plan by partial ID.
///
/// Searches for a plan file matching the given partial ID in the plans directory.
/// Returns the full path to the plan file if found, or an error if not found
/// or if multiple plans match (ambiguous).
pub async fn find_plan_by_id(partial_id: &str) -> Result<PathBuf> {
    let dir = plans_dir();

    // Validate ID before any path construction
    validate_id(partial_id, EntityKind::Plan)?;

    // Use store as authoritative source when available; filesystem fallback only when store fails
    match get_or_init_store().await {
        Ok(store) => {
            // Exact match check - does file exist on disk?
            let exact_match_path = dir.join(format!("{partial_id}.md"));
            if exact_match_path.exists() {
                return Ok(exact_match_path);
            }

            // Partial match via store (store is authoritative, no filesystem fallback)
            let matches = store.find_plan_by_partial_id(partial_id);
            match matches.len() {
                0 => Err(JanusError::PlanNotFound(partial_id.to_string())),
                1 => Ok(dir.join(format!("{}.md", &matches[0]))),
                _ => Err(JanusError::AmbiguousPlanId(partial_id.to_string(), matches)),
            }
        }
        Err(_) => {
            // FALLBACK: File-based implementation only when store is unavailable
            find_plan_by_id_filesystem(partial_id, &dir)
        }
    }
}

/// Filesystem-based find implementation for plans (fallback when store unavailable).
fn find_plan_by_id_filesystem(partial_id: &str, dir: &std::path::Path) -> Result<PathBuf> {
    let files = DirScanner::find_markdown_files_from_path(dir).unwrap_or_else(|e| {
        eprintln!("Warning: failed to read {} directory: {}", dir.display(), e);
        Vec::new()
    });

    // Check for exact match first
    let exact_name = format!("{partial_id}.md");
    if files.iter().any(|f| f == &exact_name) {
        return Ok(dir.join(&exact_name));
    }

    // Then check for partial matches
    let matches: Vec<_> = files.iter().filter(|f| f.contains(partial_id)).collect();

    match matches.len() {
        0 => Err(JanusError::PlanNotFound(partial_id.to_string())),
        1 => Ok(dir.join(matches[0])),
        _ => Err(JanusError::AmbiguousPlanId(
            partial_id.to_string(),
            matches.iter().map(|m| m.replace(".md", "")).collect(),
        )),
    }
}

/// A plan handle for reading and writing plan files.
///
/// Plans are stored as Markdown files with YAML frontmatter in `.janus/plans/`.
/// This struct provides direct file I/O operations for reading and writing plan files,
/// with built-in support for hooks.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Path to the plan file
    pub file_path: PathBuf,
    /// Plan ID
    pub id: String,
}

impl Plan {
    /// Find a plan by its (partial) ID
    pub async fn find(partial_id: &str) -> Result<Self> {
        let file_path = find_plan_by_id(partial_id).await?;
        let id = extract_id_from_path(&file_path, "plan")?;
        Ok(Plan { file_path, id })
    }

    /// Create a plan handle for a given file path
    pub fn new(file_path: PathBuf) -> Result<Self> {
        let id = extract_id_from_path(&file_path, "plan")?;
        Ok(Plan { file_path, id })
    }

    /// Create a plan handle for a new plan with the given ID
    pub fn with_id(id: &str) -> Result<Self> {
        // Validate that ID contains only alphanumeric characters and hyphens
        if !id.chars().all(|c| c.is_alphanumeric() || c == '-') {
            return Err(JanusError::InvalidPlanId(id.to_string()));
        }
        let file_path = plans_dir().join(format!("{id}.md"));
        Ok(Plan {
            file_path,
            id: id.to_string(),
        })
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
        let content = self.read_content()?;
        let mut metadata = parse_plan_content(&content)?;
        metadata.file_path = Some(self.file_path.clone());
        Ok(metadata)
    }

    /// Read the raw content of the plan file
    pub fn read_content(&self) -> Result<String> {
        fs::read_to_string(&self.file_path)
            .map_err(|e| format_file_error(&self.file_path, FileOperation::Read, "plan", e))
    }

    /// Write content to the plan file
    ///
    /// This method triggers `PreWrite` hook before writing, and `PostWrite` + `PlanUpdated`
    /// hooks after successful write.
    pub fn write(&self, content: &str) -> Result<()> {
        crate::fs::with_write_hooks(
            self.hook_context(),
            || self.write_raw(content),
            Some(HookEvent::PlanUpdated),
        )
    }

    /// Write content to the plan file with validation
    ///
    /// Validates the content can be parsed as a plan before writing.
    pub fn write_validated(&self, content: &str) -> Result<()> {
        parse_plan_content(content)?;
        self.write(content)
    }

    /// Write the given metadata to the plan file
    pub fn write_metadata(&self, metadata: &PlanMetadata) -> Result<()> {
        let content = serialize_plan(metadata);
        self.write(&content)
    }

    /// Write content to the plan file without triggering hooks.
    ///
    /// Used internally when hooks should be handled at a higher level
    /// (e.g., plan creation where PlanCreated should be fired instead of PlanUpdated).
    pub(crate) fn write_without_hooks(&self, content: &str) -> Result<()> {
        self.write_raw(content)
    }

    /// Write raw content without hooks
    fn write_raw(&self, content: &str) -> Result<()> {
        self.ensure_parent_dir()?;
        fs::write(&self.file_path, content)
            .map_err(|e| format_file_error(&self.file_path, FileOperation::Write, "plan", e))
    }

    /// Ensure the parent directory exists
    fn ensure_parent_dir(&self) -> Result<()> {
        crate::fs::ensure_parent_dir(&self.file_path)
    }

    /// Delete the plan file
    ///
    /// This method triggers `PreDelete` hook before deletion, and `PostDelete` + `PlanDeleted`
    /// hooks after successful deletion.
    pub fn delete(&self) -> Result<()> {
        if !self.file_path.exists() {
            return Ok(());
        }

        let context = self.hook_context();

        run_pre_hooks(HookEvent::PreDelete, &context)?;

        fs::remove_file(&self.file_path)
            .map_err(|e| format_file_error(&self.file_path, FileOperation::Delete, "plan", e))?;

        run_post_hooks(HookEvent::PostDelete, &context);
        run_post_hooks(HookEvent::PlanDeleted, &context);

        Ok(())
    }

    /// Build a hook context for this plan
    pub fn hook_context(&self) -> HookContext {
        HookContext::new()
            .with_item_type(EntityType::Plan)
            .with_item_id(&self.id)
            .with_file_path(&self.file_path)
    }

    /// Compute the status of this plan based on its tickets
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
    pub fn compute_all_phase_statuses(
        &self,
        ticket_map: &HashMap<String, TicketMetadata>,
    ) -> Result<Vec<PhaseStatus>> {
        let metadata = self.read()?;
        Ok(compute_all_phase_statuses(&metadata, ticket_map))
    }
}

impl Entity for Plan {
    type Metadata = PlanMetadata;

    async fn find(partial_id: &str) -> Result<Self> {
        Plan::find(partial_id).await
    }

    fn read(&self) -> Result<PlanMetadata> {
        self.read()
    }

    fn write(&self, content: &str) -> Result<()> {
        self.write(content)
    }

    fn delete(&self) -> Result<()> {
        self.delete()
    }

    fn exists(&self) -> bool {
        self.exists()
    }
}

/// Get all plans from the in-memory store.
///
/// The store is populated from disk on first access and kept
/// up-to-date via the filesystem watcher.
pub async fn get_all_plans() -> Result<PlanLoadResult> {
    match crate::cache::get_or_init_store().await {
        Ok(store) => {
            let plans = store.get_all_plans();
            let mut result = PlanLoadResult::new();
            for plan in plans {
                result.add_plan(plan);
            }
            Ok(result)
        }
        Err(_) => {
            // Fall back to disk reads if store initialization fails
            Ok(get_all_plans_from_disk())
        }
    }
}

/// Build a HashMap by ID from all plans
pub async fn build_plan_map() -> Result<HashMap<String, PlanMetadata>> {
    let result = get_all_plans().await?;
    let map: HashMap<_, _> = result
        .items
        .into_iter()
        .filter_map(|m| m.id.clone().map(|id| (id, m)))
        .collect();
    Ok(map)
}

/// Get all plans and the map together (efficient single call)
pub async fn get_all_plans_with_map() -> Result<(PlanLoadResult, HashMap<String, PlanMetadata>)> {
    let result = get_all_plans().await?;
    let map: HashMap<_, _> = result
        .items
        .iter()
        .filter_map(|m| m.id.clone().map(|id| (id, m.clone())))
        .collect();
    Ok((result, map))
}

/// Get all plans from disk (fallback implementation)
pub fn get_all_plans_from_disk() -> PlanLoadResult {
    let files = find_plans();
    let mut result = PlanLoadResult::new();
    let p_dir = plans_dir();

    for file in files {
        let file_path = p_dir.join(&file);
        match fs::read_to_string(&file_path) {
            Ok(content) => match parse_plan_content(&content) {
                Ok(mut metadata) => {
                    if metadata.id.is_none() {
                        metadata.id = Some(file.strip_suffix(".md").unwrap_or(&file).to_string());
                    }
                    metadata.file_path = Some(file_path);
                    result.add_plan(metadata);
                }
                Err(e) => {
                    result.add_failure(&file, format!("parse error: {e}"));
                }
            },
            Err(e) => {
                result.add_failure(&file, format!("read error: {e}"));
            }
        }
    }

    result
}

/// Ensure the plans directory exists
pub fn ensure_plans_dir() -> Result<()> {
    let p_dir = plans_dir();
    fs::create_dir_all(&p_dir).map_err(|e| {
        JanusError::Io(std::io::Error::new(
            e.kind(),
            format!(
                "Failed to create plans directory at {}: {}",
                crate::utils::format_relative_path(&p_dir),
                e
            ),
        ))
    })
}

/// Generate a unique plan ID with collision checking
pub fn generate_plan_id() -> String {
    use crate::utils::generate_hash;

    const RETRIES_PER_LENGTH: u32 = 40;
    let p_dir = plans_dir();

    for length in 4..=8 {
        for _ in 0..RETRIES_PER_LENGTH {
            let hash = generate_hash(length);
            let candidate = format!("plan-{hash}");
            let filename = format!("{candidate}.md");

            if !p_dir.join(&filename).exists() {
                return candidate;
            }
        }
    }

    panic!(
        "Failed to generate unique plan ID after trying hash lengths 4-8 with {RETRIES_PER_LENGTH} retries each"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plan_via_parser() {
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
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "plan");
        assert_eq!(parts[1].len(), 4);
        assert!(parts[1].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_plan_with_id() {
        let plan = Plan::with_id("plan-test").unwrap();
        assert_eq!(plan.id, "plan-test");
        assert_eq!(plan.file_path, PathBuf::from(".janus/plans/plan-test.md"));
    }

    #[test]
    fn test_plan_with_id_invalid() {
        let result = Plan::with_id("../../../etc/passwd");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::InvalidPlanId(id) => {
                assert_eq!(id, "../../../etc/passwd");
            }
            _ => panic!("Expected InvalidPlanId error"),
        }
    }

    #[test]
    fn test_plan_new_valid_path() {
        let path = PathBuf::from(".janus/plans/plan-abc123.md");
        let plan = Plan::new(path).unwrap();
        assert_eq!(plan.id, "plan-abc123");
    }

    #[test]
    fn test_plan_new_invalid_path_no_stem() {
        let path = PathBuf::from("/");
        let result = Plan::new(path);
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::InvalidFormat(msg) => {
                assert!(msg.contains("Invalid plan file path"));
            }
            _ => panic!("Expected InvalidFormat error"),
        }
    }

    #[test]
    fn test_plan_new_invalid_path_empty_stem() {
        let path = PathBuf::from("");
        let result = Plan::new(path);
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::InvalidFormat(msg) => {
                assert!(msg.contains("Invalid plan file path"));
            }
            _ => panic!("Expected InvalidFormat error"),
        }
    }

    #[test]
    fn test_plan_find_construction() {
        // Test that Plan::find would construct correctly (can't test async here easily)
        let path = PathBuf::from(".janus/plans/plan-xyz789.md");
        let plan = Plan::new(path).unwrap();
        assert_eq!(plan.id, "plan-xyz789");
        assert!(plan.file_path.to_string_lossy().contains("plan-xyz789"));
    }
}
