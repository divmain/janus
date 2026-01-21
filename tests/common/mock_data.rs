//! Mock data builders for creating test tickets and other entities.
//!
//! This module provides builder patterns for creating test data without
//! needing to read from disk.

use janus::types::{TicketMetadata, TicketPriority, TicketStatus, TicketType};

/// Builder for creating test tickets
pub struct TicketBuilder {
    metadata: TicketMetadata,
}

impl TicketBuilder {
    /// Create a new ticket builder with the given ID
    pub fn new(id: &str) -> Self {
        Self {
            metadata: TicketMetadata {
                id: Some(id.to_string()),
                status: Some(TicketStatus::New),
                ticket_type: Some(TicketType::Task),
                priority: Some(TicketPriority::P2),
                created: Some("2024-01-01T00:00:00Z".to_string()),
                ..Default::default()
            },
        }
    }

    /// Set the ticket title
    pub fn title(mut self, title: &str) -> Self {
        self.metadata.title = Some(title.to_string());
        self
    }

    /// Set the ticket status
    pub fn status(mut self, status: TicketStatus) -> Self {
        self.metadata.status = Some(status);
        self
    }

    /// Set the ticket type
    pub fn ticket_type(mut self, t: TicketType) -> Self {
        self.metadata.ticket_type = Some(t);
        self
    }

    /// Set the ticket priority
    pub fn priority(mut self, p: TicketPriority) -> Self {
        self.metadata.priority = Some(p);
        self
    }

    /// Add a dependency
    pub fn dep(mut self, dep_id: &str) -> Self {
        self.metadata.deps.push(dep_id.to_string());
        self
    }

    /// Set the parent ticket
    pub fn parent(mut self, parent_id: &str) -> Self {
        self.metadata.parent = Some(parent_id.to_string());
        self
    }

    /// Build the ticket metadata
    pub fn build(self) -> TicketMetadata {
        self.metadata
    }
}

/// Create a basic ticket with minimal setup
pub fn mock_ticket(id: &str, status: TicketStatus) -> TicketMetadata {
    TicketBuilder::new(id)
        .title(&format!("Test ticket {}", id))
        .status(status)
        .build()
}

/// Create multiple tickets with the given statuses
pub fn mock_tickets(specs: &[(&str, TicketStatus)]) -> Vec<TicketMetadata> {
    specs
        .iter()
        .map(|(id, status)| mock_ticket(id, *status))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ticket_builder_basic() {
        let ticket = TicketBuilder::new("j-test").build();
        assert_eq!(ticket.id, Some("j-test".to_string()));
        assert_eq!(ticket.status, Some(TicketStatus::New));
    }

    #[test]
    fn test_ticket_builder_with_all_fields() {
        let ticket = TicketBuilder::new("j-test")
            .title("Test Title")
            .status(TicketStatus::InProgress)
            .ticket_type(TicketType::Bug)
            .priority(TicketPriority::P0)
            .dep("j-dep1")
            .parent("j-parent")
            .build();

        assert_eq!(ticket.id, Some("j-test".to_string()));
        assert_eq!(ticket.title, Some("Test Title".to_string()));
        assert_eq!(ticket.status, Some(TicketStatus::InProgress));
        assert_eq!(ticket.ticket_type, Some(TicketType::Bug));
        assert_eq!(ticket.priority, Some(TicketPriority::P0));
        assert_eq!(ticket.deps, vec!["j-dep1"]);
        assert_eq!(ticket.parent, Some("j-parent".to_string()));
    }

    #[test]
    fn test_mock_ticket() {
        let ticket = mock_ticket("j-123", TicketStatus::Complete);
        assert_eq!(ticket.id, Some("j-123".to_string()));
        assert_eq!(ticket.status, Some(TicketStatus::Complete));
        assert!(ticket.title.unwrap().contains("j-123"));
    }

    #[test]
    fn test_mock_tickets() {
        let tickets = mock_tickets(&[("j-1", TicketStatus::New), ("j-2", TicketStatus::Complete)]);
        assert_eq!(tickets.len(), 2);
        assert_eq!(tickets[0].status, Some(TicketStatus::New));
        assert_eq!(tickets[1].status, Some(TicketStatus::Complete));
    }
}
