//! Status computation module.
//!
//! This module provides unified status computation logic for both tickets and plans.
//! All status-related predicates, aggregations, and computations are centralized here.

use std::collections::HashMap;

use crate::types::{TicketMetadata, TicketStatus};

pub mod plan;

pub use plan::{
    compute_aggregate_status, compute_all_phase_statuses, compute_phase_status,
    compute_plan_status, resolve_ticket_or_warn,
};

/// Returns true if a status represents a terminal state (complete or cancelled).
///
/// Terminal states indicate no further work is expected on the ticket.
pub const fn is_terminal(status: TicketStatus) -> bool {
    matches!(status, TicketStatus::Complete | TicketStatus::Cancelled)
}

/// Returns true if a status indicates work has not yet started (new or next).
///
/// These are pre-work states where the ticket is queued but not actively being worked on.
pub const fn is_not_started(status: TicketStatus) -> bool {
    matches!(status, TicketStatus::New | TicketStatus::Next)
}

/// Canonical check for whether a single dependency is satisfied.
///
/// A dependency is satisfied when:
/// - The dep ticket exists in the ticket map AND has a terminal status (Complete or Cancelled)
///
/// A dependency is NOT satisfied (blocking) when:
/// - The dep ticket exists but has a non-terminal status
/// - The dep ticket does not exist in the ticket map (orphan/dangling dep) — this is the
///   safer default, since a missing ticket cannot be verified as done
pub fn is_dependency_satisfied(dep_id: &str, ticket_map: &HashMap<String, TicketMetadata>) -> bool {
    ticket_map
        .get(dep_id)
        .is_some_and(|dep| dep.status.is_some_and(|s| s.is_terminal()))
}

/// Check whether ALL dependencies of a ticket are satisfied.
///
/// Returns true if the ticket has no deps, or every dep is satisfied per
/// [`is_dependency_satisfied`].
pub fn all_deps_satisfied(
    ticket: &TicketMetadata,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> bool {
    ticket
        .deps
        .iter()
        .all(|dep_id| is_dependency_satisfied(dep_id, ticket_map))
}

/// Check whether ANY dependency of a ticket is unsatisfied (blocking).
///
/// Returns true if at least one dep is NOT satisfied per [`is_dependency_satisfied`].
/// Returns false if the ticket has no deps.
pub fn has_unsatisfied_dep(
    ticket: &TicketMetadata,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> bool {
    ticket
        .deps
        .iter()
        .any(|dep_id| !is_dependency_satisfied(dep_id, ticket_map))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TicketId;

    fn make_ticket(id: &str, status: TicketStatus, deps: Vec<&str>) -> TicketMetadata {
        TicketMetadata {
            id: Some(TicketId::new_unchecked(id)),
            status: Some(status),
            deps: deps
                .into_iter()
                .map(|s| TicketId::new_unchecked(s))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn test_is_terminal() {
        assert!(is_terminal(TicketStatus::Complete));
        assert!(is_terminal(TicketStatus::Cancelled));
        assert!(!is_terminal(TicketStatus::New));
        assert!(!is_terminal(TicketStatus::Next));
        assert!(!is_terminal(TicketStatus::InProgress));
    }

    #[test]
    fn test_is_not_started() {
        assert!(is_not_started(TicketStatus::New));
        assert!(is_not_started(TicketStatus::Next));
        assert!(!is_not_started(TicketStatus::InProgress));
        assert!(!is_not_started(TicketStatus::Complete));
        assert!(!is_not_started(TicketStatus::Cancelled));
    }

    #[test]
    fn test_is_dependency_satisfied_complete() {
        let mut map = HashMap::new();
        map.insert(
            "j-dep".to_string(),
            make_ticket("j-dep", TicketStatus::Complete, vec![]),
        );
        assert!(is_dependency_satisfied("j-dep", &map));
    }

    #[test]
    fn test_is_dependency_satisfied_cancelled() {
        let mut map = HashMap::new();
        map.insert(
            "j-dep".to_string(),
            make_ticket("j-dep", TicketStatus::Cancelled, vec![]),
        );
        assert!(is_dependency_satisfied("j-dep", &map));
    }

    #[test]
    fn test_is_dependency_not_satisfied_new() {
        let mut map = HashMap::new();
        map.insert(
            "j-dep".to_string(),
            make_ticket("j-dep", TicketStatus::New, vec![]),
        );
        assert!(!is_dependency_satisfied("j-dep", &map));
    }

    #[test]
    fn test_is_dependency_not_satisfied_in_progress() {
        let mut map = HashMap::new();
        map.insert(
            "j-dep".to_string(),
            make_ticket("j-dep", TicketStatus::InProgress, vec![]),
        );
        assert!(!is_dependency_satisfied("j-dep", &map));
    }

    #[test]
    fn test_is_dependency_not_satisfied_orphan() {
        let map = HashMap::new();
        // Orphan dep should NOT be satisfied (safer default)
        assert!(!is_dependency_satisfied("j-nonexistent", &map));
    }

    #[test]
    fn test_all_deps_satisfied_no_deps() {
        let map = HashMap::new();
        let ticket = make_ticket("j-a", TicketStatus::New, vec![]);
        assert!(all_deps_satisfied(&ticket, &map));
    }

    #[test]
    fn test_all_deps_satisfied_all_complete() {
        let mut map = HashMap::new();
        map.insert(
            "j-b".to_string(),
            make_ticket("j-b", TicketStatus::Complete, vec![]),
        );
        map.insert(
            "j-c".to_string(),
            make_ticket("j-c", TicketStatus::Cancelled, vec![]),
        );
        let ticket = make_ticket("j-a", TicketStatus::New, vec!["j-b", "j-c"]);
        assert!(all_deps_satisfied(&ticket, &map));
    }

    #[test]
    fn test_all_deps_satisfied_one_incomplete() {
        let mut map = HashMap::new();
        map.insert(
            "j-b".to_string(),
            make_ticket("j-b", TicketStatus::Complete, vec![]),
        );
        map.insert(
            "j-c".to_string(),
            make_ticket("j-c", TicketStatus::New, vec![]),
        );
        let ticket = make_ticket("j-a", TicketStatus::New, vec!["j-b", "j-c"]);
        assert!(!all_deps_satisfied(&ticket, &map));
    }

    #[test]
    fn test_all_deps_satisfied_orphan_blocks() {
        let mut map = HashMap::new();
        map.insert(
            "j-b".to_string(),
            make_ticket("j-b", TicketStatus::Complete, vec![]),
        );
        let ticket = make_ticket("j-a", TicketStatus::New, vec!["j-b", "j-missing"]);
        assert!(!all_deps_satisfied(&ticket, &map));
    }

    #[test]
    fn test_has_unsatisfied_dep_no_deps() {
        let map = HashMap::new();
        let ticket = make_ticket("j-a", TicketStatus::New, vec![]);
        assert!(!has_unsatisfied_dep(&ticket, &map));
    }

    #[test]
    fn test_has_unsatisfied_dep_one_blocking() {
        let mut map = HashMap::new();
        map.insert(
            "j-b".to_string(),
            make_ticket("j-b", TicketStatus::InProgress, vec![]),
        );
        let ticket = make_ticket("j-a", TicketStatus::New, vec!["j-b"]);
        assert!(has_unsatisfied_dep(&ticket, &map));
    }
}
