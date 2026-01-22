//! Types for view handler actions
//!
//! This module defines the action types that handlers can send to the async
//! queue for processing.

/// Actions that can be sent to the async processing queue
#[derive(Debug, Clone)]
pub enum ViewAction {
    /// Cycle a ticket's status to the next value
    CycleStatus {
        /// Ticket ID to update
        id: String,
    },
    /// Load a ticket for editing
    LoadForEdit {
        /// Ticket ID to load
        id: String,
    },
    /// Mark a ticket as triaged
    MarkTriaged {
        /// Ticket ID to triage
        id: String,
        /// Whether to mark as triaged (true) or not triaged (false)
        triaged: bool,
    },
    /// Cancel a ticket
    CancelTicket {
        /// Ticket ID to cancel
        id: String,
    },
    /// Add a note to a ticket
    AddNote {
        /// Ticket ID to add note to
        id: String,
        /// The note text to add
        note: String,
    },
}
