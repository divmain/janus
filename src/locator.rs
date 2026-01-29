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
    tickets_items_dir().join(format!("{}.md", id))
}

/// Returns the file path for a plan with the given ID.
///
/// The path is constructed as `<plans_dir>/<id>.md`.
/// This function does not verify that the file exists.
pub fn plan_path(id: &str) -> PathBuf {
    plans_dir().join(format!("{}.md", id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_ticket_path() {
        // SAFETY: We use #[serial] to ensure single-threaded access
        unsafe { std::env::remove_var("JANUS_ROOT") };
        let path = ticket_path("j-a1b2");
        assert!(path.to_string_lossy().contains(".janus/items"));
        assert!(path.to_string_lossy().contains("j-a1b2.md"));
    }

    #[test]
    #[serial]
    fn test_plan_path() {
        // SAFETY: We use #[serial] to ensure single-threaded access
        unsafe { std::env::remove_var("JANUS_ROOT") };
        let path = plan_path("plan-a1b2");
        assert!(path.to_string_lossy().contains(".janus/plans"));
        assert!(path.to_string_lossy().contains("plan-a1b2.md"));
    }

    #[test]
    #[serial]
    fn test_ticket_path_with_env_var() {
        // SAFETY: We use #[serial] to ensure single-threaded access
        unsafe { std::env::set_var("JANUS_ROOT", "/custom/path/.janus") };
        let path = ticket_path("j-test");
        assert!(path.to_string_lossy().contains("/custom/path/.janus/items"));
        assert!(path.to_string_lossy().contains("j-test.md"));
        unsafe { std::env::remove_var("JANUS_ROOT") };
    }
}
