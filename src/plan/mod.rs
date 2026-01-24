//! Plan module with focused component architecture
//!
//! This module provides the `Plan` facade type which orchestrates:
//! - `PlanLocator`: ID and path resolution
//! - `PlanFile`: File I/O with `StorageHandle` and `FileStorage` traits
//! - `PlanEditor`: Content modification with hook orchestration
//!
//! The architecture mirrors the Ticket module pattern for consistency.

mod editor;
mod file;
mod locator;
pub mod parser;
pub mod types;

pub use editor::PlanEditor;
pub use file::PlanFile;
pub use locator::PlanLocator;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use async_trait::async_trait;

use crate::cache;
use crate::error::{JanusError, Result};
use crate::finder::Findable;
use crate::hooks::{run_post_hooks, run_pre_hooks, HookContext, HookEvent};
use crate::plan::parser::parse_plan_content;
use crate::plan::types::{Phase, PhaseStatus, PlanMetadata, PlanStatus};
use crate::repository::ItemRepository;
use crate::storage::FileStorage;
use crate::types::{plans_dir, TicketMetadata};
use crate::utils::DirScanner;

// Re-export status computation functions
pub use crate::status::plan::{
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

/// Plan-specific implementation of the Findable trait
struct PlanFinder;

impl Findable for PlanFinder {
    fn directory() -> PathBuf {
        plans_dir()
    }

    fn cache_find_by_partial_id(
        cache: &cache::TicketCache,
        partial_id: &str,
    ) -> impl std::future::Future<Output = Result<Vec<String>>> + Send {
        cache.find_plan_by_partial_id(partial_id)
    }

    fn not_found_error(partial_id: String) -> JanusError {
        JanusError::PlanNotFound(partial_id)
    }

    fn ambiguous_id_error(partial_id: String, matches: Vec<String>) -> JanusError {
        JanusError::AmbiguousPlanId(partial_id, matches)
    }
}

/// Find all plan files in the plans directory
fn find_plans() -> Vec<String> {
    DirScanner::find_markdown_files(plans_dir()).unwrap_or_else(|e| {
        eprintln!("Warning: failed to read plans directory: {}", e);
        Vec::new()
    })
}

pub struct PlanRepository;

#[async_trait]
impl ItemRepository for PlanRepository {
    type Item = Plan;
    type Metadata = PlanMetadata;

    async fn get_all_static() -> Result<Vec<PlanMetadata>> {
        get_all_plans().await
    }
}

impl PlanRepository {
    /// Get all plans from the module-level function (compatibility method)
    pub async fn get_all_plans_compat() -> Result<Vec<PlanMetadata>> {
        get_all_plans().await
    }

    /// Build a HashMap by ID
    pub async fn build_plan_map() -> Result<HashMap<String, PlanMetadata>> {
        <Self as ItemRepository>::build_map_static().await
    }

    /// Get all plans and the map together (efficient single call)
    pub async fn get_all_with_map() -> Result<(Vec<PlanMetadata>, HashMap<String, PlanMetadata>)> {
        <Self as ItemRepository>::get_all_with_map_static().await
    }
}

/// Find a plan file by partial ID
pub async fn find_plan_by_id(partial_id: &str) -> Result<PathBuf> {
    crate::finder::find_by_partial_id::<PlanFinder>(partial_id).await
}

/// A plan handle for reading and writing plan files.
///
/// `Plan` is a facade that orchestrates the focused components:
/// - `PlanLocator`: ID and path resolution
/// - `PlanFile`: File I/O with `StorageHandle` and `FileStorage` traits
/// - `PlanEditor`: Content modification with hook orchestration
///
/// The public fields `file_path` and `id` are maintained for backward compatibility.
pub struct Plan {
    /// Path to the plan file (for backward compatibility)
    pub file_path: PathBuf,
    /// Plan ID (for backward compatibility)
    pub id: String,
    /// Internal file handle for I/O operations
    file: PlanFile,
    /// Internal editor for content modifications
    editor: PlanEditor,
}

impl std::fmt::Debug for Plan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plan")
            .field("file_path", &self.file_path)
            .field("id", &self.id)
            .finish()
    }
}

impl Plan {
    /// Find a plan by its (partial) ID
    pub async fn find(partial_id: &str) -> Result<Self> {
        let locator = PlanLocator::find(partial_id).await?;
        Ok(Self::from_locator(locator))
    }

    /// Create a plan handle for a given file path
    pub fn new(file_path: PathBuf) -> Result<Self> {
        let locator = PlanLocator::new(file_path)?;
        Ok(Self::from_locator(locator))
    }

    /// Create a plan handle for a new plan with the given ID
    pub fn with_id(id: &str) -> Self {
        let locator = PlanLocator::with_id(id);
        Self::from_locator(locator)
    }

    /// Internal constructor from a locator
    fn from_locator(locator: PlanLocator) -> Self {
        let file = PlanFile::new(locator.clone());
        let editor = PlanEditor::new(file.clone());
        Plan {
            file_path: locator.file_path.clone(),
            id: locator.id.clone(),
            file,
            editor,
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
        let content = self.file.read_raw()?;
        let mut metadata = parse_plan_content(&content)?;
        metadata.file_path = Some(self.file_path.clone());
        Ok(metadata)
    }

    /// Read the raw content of the plan file
    pub fn read_content(&self) -> Result<String> {
        self.file.read_raw()
    }

    /// Write content to the plan file
    ///
    /// This method triggers `PreWrite` hook before writing, and `PostWrite` + `PlanUpdated`
    /// hooks after successful write.
    pub fn write(&self, content: &str) -> Result<()> {
        self.editor.write(content)
    }

    /// Write content to the plan file without triggering hooks.
    ///
    /// Used internally when hooks should be handled at a higher level
    /// (e.g., plan creation where PlanCreated should be fired instead of PlanUpdated).
    pub(crate) fn write_without_hooks(&self, content: &str) -> Result<()> {
        self.editor.write_without_hooks(content)
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
        let context = self.hook_context();

        // Run pre-delete hook (can abort)
        run_pre_hooks(HookEvent::PreDelete, &context)?;

        // Perform the delete
        fs::remove_file(&self.file_path).map_err(|e| {
            JanusError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to delete plan at {}: {}",
                    self.file_path.display(),
                    e
                ),
            ))
        })?;

        // Run post-delete hooks (fire-and-forget)
        run_post_hooks(HookEvent::PostDelete, &context);
        run_post_hooks(HookEvent::PlanDeleted, &context);

        Ok(())
    }

    /// Build a hook context for this plan.
    ///
    /// This is a convenience method to avoid repeating the same hook context
    /// construction pattern throughout the codebase.
    pub fn hook_context(&self) -> HookContext {
        self.file.hook_context()
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
pub async fn get_all_plans() -> Result<Vec<PlanMetadata>> {
    // Try cache first
    if let Some(cache) = cache::get_or_init_cache().await {
        if let Ok(cached_plans) = cache.get_all_plans().await {
            // Convert cached plans to full PlanMetadata
            let mut plans = Vec::new();
            let p_dir = plans_dir();
            for cached in cached_plans {
                if let Some(id) = &cached.id {
                    let file_path = p_dir.join(format!("{}.md", id));
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
                return Ok(get_all_plans_from_disk());
            }
            return Ok(plans);
        }
        eprintln!("Warning: cache read failed, falling back to file reads");
    }

    // FALLBACK: Original implementation
    Ok(get_all_plans_from_disk())
}

/// Get all plans from disk (fallback implementation)
pub fn get_all_plans_from_disk() -> Vec<PlanMetadata> {
    let files = find_plans();
    let mut plans = Vec::new();
    let p_dir = plans_dir();

    for file in files {
        let file_path = p_dir.join(&file);
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
pub fn ensure_plans_dir() -> Result<()> {
    let p_dir = plans_dir();
    fs::create_dir_all(&p_dir).map_err(|e| {
        JanusError::Io(std::io::Error::new(
            e.kind(),
            format!(
                "Failed to create plans directory at {}: {}",
                p_dir.display(),
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
            let candidate = format!("plan-{}", hash);
            let filename = format!("{}.md", candidate);

            if !p_dir.join(&filename).exists() {
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
        // Empty path has no file_stem
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
}
