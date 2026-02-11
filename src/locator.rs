//! Simple path utilities for tickets and plans
//!
//! This module provides simple functions for constructing file paths
//! from entity IDs. For finding entities by partial ID, use the
//! entity-specific find functions (e.g., `find_ticket_by_id`).

use std::path::PathBuf;

use crate::types::{plans_dir, tickets_items_dir};

/// Returns the file path for a ticket with the given ID.
///
/// The path is constructed as `<tickets_dir>/<id>.md`.
/// This function does not verify that the file exists.
pub fn ticket_path(id: &str) -> PathBuf {
    tickets_items_dir().join(format!("{id}.md"))
}

/// Returns the file path for a plan with the given ID.
///
/// The path is constructed as `<plans_dir>/<id>.md`.
/// This function does not verify that the file exists.
pub fn plan_path(id: &str) -> PathBuf {
    plans_dir().join(format!("{id}.md"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::paths::JanusRootGuard;

    #[test]
    fn test_ticket_path() {
        let _guard = JanusRootGuard::new(".janus");
        let path = ticket_path("j-a1b2");
        assert!(path.to_string_lossy().contains(".janus/items"));
        assert!(path.to_string_lossy().contains("j-a1b2.md"));
    }

    #[test]
    fn test_plan_path() {
        let _guard = JanusRootGuard::new(".janus");
        let path = plan_path("plan-a1b2");
        assert!(path.to_string_lossy().contains(".janus/plans"));
        assert!(path.to_string_lossy().contains("plan-a1b2.md"));
    }

    #[test]
    fn test_ticket_path_with_env_var() {
        let _guard = JanusRootGuard::new("/custom/path/.janus");
        let path = ticket_path("j-test");
        assert!(path.to_string_lossy().contains("/custom/path/.janus/items"));
        assert!(path.to_string_lossy().contains("j-test.md"));
    }
}
