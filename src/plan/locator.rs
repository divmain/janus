//! Plan ID and path resolution
//!
//! Provides the `PlanLocator` struct for resolving plan paths from IDs
//! and vice versa. Follows the same pattern as `TicketLocator`.

use crate::locator::Locator;
use crate::locator::PlanEntity;

/// Type alias for plan locator using the generic Locator
pub type PlanLocator = Locator<PlanEntity>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
