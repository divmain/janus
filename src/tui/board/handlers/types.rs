//! Types for board handler actions
//!
//! This module defines the action types that handlers can send to the async
//! queue for processing.

use crate::types::TicketStatus;

/// Actions that can be sent to the async processing queue
#[derive(Debug, Clone)]
pub enum TicketAction {
    /// Update a ticket's status
    UpdateStatus {
        /// Ticket ID to update
        id: String,
        /// New status to set
        status: TicketStatus,
    },
    /// Load a ticket for editing
    LoadForEdit {
        /// Ticket ID to load
        id: String,
    },
}
