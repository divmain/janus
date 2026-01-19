//! Status computation module.
//!
//! This module provides unified status computation logic for both tickets and plans.
//! All status-related predicates, aggregations, and computations are centralized here.

use crate::types::TicketStatus;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
