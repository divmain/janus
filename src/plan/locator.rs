//! Plan ID and path resolution
//!
//! Provides the `PlanLocator` struct for resolving plan paths from IDs
//! and vice versa. Follows the same pattern as `TicketLocator`.

use std::path::PathBuf;

use crate::error::Result;
use crate::types::plans_dir;
use crate::utils::extract_id_from_path;

/// Handles plan ID and file path resolution.
///
/// The locator encapsulates the relationship between a plan's ID and its
/// file path on disk. Plans are stored as `{id}.md` in the plans directory.
#[derive(Debug, Clone)]
pub struct PlanLocator {
    pub file_path: PathBuf,
    pub id: String,
}

impl PlanLocator {
    /// Create a locator from an existing file path.
    ///
    /// Extracts the plan ID from the file path's stem.
    ///
    /// # Arguments
    /// * `file_path` - Path to the plan file (e.g., `.janus/plans/plan-a1b2.md`)
    ///
    /// # Returns
    /// A `PlanLocator` with the extracted ID, or an error if the path is invalid.
    pub fn new(file_path: PathBuf) -> Result<Self> {
        let id = extract_id_from_path(&file_path, "plan")?;
        Ok(PlanLocator { file_path, id })
    }

    /// Find a plan by its (partial) ID.
    ///
    /// Searches for a plan matching the given partial ID. If exactly one match
    /// is found, returns a locator for that plan. Returns an error if no match
    /// is found or if the ID is ambiguous.
    ///
    /// # Arguments
    /// * `partial_id` - A full or partial plan ID (e.g., "plan-a1", "a1b2")
    pub async fn find(partial_id: &str) -> Result<Self> {
        let file_path = super::find_plan_by_id(partial_id).await?;
        PlanLocator::new(file_path)
    }

    /// Get the file path for a given plan ID.
    ///
    /// Constructs the expected file path for a plan with the given ID.
    /// Does not verify that the file exists.
    ///
    /// # Arguments
    /// * `id` - The full plan ID (e.g., "plan-a1b2")
    pub fn file_path_for_id(id: &str) -> PathBuf {
        plans_dir().join(format!("{}.md", id))
    }

    /// Create a locator for a new plan with the given ID.
    ///
    /// This is used when creating new plans. The file does not need to exist.
    ///
    /// # Arguments
    /// * `id` - The plan ID to use
    pub fn with_id(id: &str) -> Self {
        PlanLocator {
            file_path: Self::file_path_for_id(id),
            id: id.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_locator_new_valid_path() {
        let path = PathBuf::from("/path/to/plan-a1b2.md");
        let result = PlanLocator::new(path.clone());
        assert!(result.is_ok());
        let locator = result.unwrap();
        assert_eq!(locator.id, "plan-a1b2");
        assert_eq!(locator.file_path, path);
    }

    #[test]
    fn test_plan_locator_new_invalid_empty_path() {
        let path = PathBuf::from("");
        let result = PlanLocator::new(path);
        assert!(result.is_err());
    }

    #[test]
    fn test_plan_locator_file_path_for_id() {
        let path = PlanLocator::file_path_for_id("plan-test");
        assert!(path.ends_with("plan-test.md"));
        assert!(path.to_string_lossy().contains("plans"));
    }

    #[test]
    fn test_plan_locator_with_id() {
        let locator = PlanLocator::with_id("plan-test");
        assert_eq!(locator.id, "plan-test");
        assert!(locator.file_path.ends_with("plan-test.md"));
    }
}
