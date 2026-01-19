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
}
