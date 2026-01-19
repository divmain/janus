//! Service for ticket edit operations
//!
//! This module provides a service layer for editing tickets, handling both
//! new ticket creation and existing ticket updates in a unified interface.

use crate::error::Result;
use crate::tui::services::TicketService;
use crate::types::{TicketPriority, TicketStatus, TicketType};

/// Service for ticket edit operations
///
/// Wraps the lower-level TicketService to provide a unified interface for
/// both creating new tickets and updating existing ones.
pub struct TicketEditService;

impl TicketEditService {
    /// Save ticket data (create new or update existing)
    ///
    /// If ticket_id is Some, updates the existing ticket.
    /// If ticket_id is None, creates a new ticket.
    pub async fn save(
        ticket_id: Option<&str>,
        title: &str,
        status: TicketStatus,
        ticket_type: TicketType,
        priority: TicketPriority,
        body: &str,
    ) -> Result<()> {
        if let Some(id) = ticket_id {
            TicketService::update_ticket(id, title, status, ticket_type, priority, body).await?;
        } else {
            TicketService::create_ticket(title, status, ticket_type, priority, body)?;
        }
        Ok(())
    }

    /// Check if this is a new ticket (no ID provided)
    pub fn is_new_ticket(ticket_id: Option<&str>) -> bool {
        ticket_id.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_new_ticket() {
        assert!(TicketEditService::is_new_ticket(None));
        assert!(!TicketEditService::is_new_ticket(Some("j-1234")));
    }
}
