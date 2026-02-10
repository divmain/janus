//! Ticket analytics for filtering, counting, and status analysis

use crate::types::{TicketData, TicketMetadata, TicketStatus};

/// Analytics operations on ticket collections
pub struct TicketAnalytics;

impl TicketAnalytics {
    /// Filter tickets by status
    pub fn tickets_by_status(
        tickets: &[TicketMetadata],
        status: TicketStatus,
    ) -> Vec<&TicketMetadata> {
        tickets
            .iter()
            .filter(|t| t.status == Some(status))
            .collect()
    }

    /// Get the total count of tickets
    pub fn ticket_count(tickets: &[TicketMetadata]) -> usize {
        tickets.len()
    }

    /// Get counts for each status (for kanban board column headers)
    pub fn status_counts(tickets: &[TicketMetadata]) -> StatusCounts {
        let mut counts = StatusCounts::default();
        for ticket in tickets {
            match ticket.status {
                Some(TicketStatus::New) => counts.new += 1,
                Some(TicketStatus::Next) => counts.next += 1,
                Some(TicketStatus::InProgress) => counts.in_progress += 1,
                Some(TicketStatus::Complete) => counts.complete += 1,
                Some(TicketStatus::Cancelled) => counts.cancelled += 1,
                None => counts.new += 1, // Default to new
            }
        }
        counts
    }

    /// Sort tickets by priority (ascending), then by ID (in-place)
    pub fn sort_by_priority(tickets: &mut [TicketMetadata]) {
        tickets.sort_by(|a, b| {
            let pa = a.priority_num();
            let pb = b.priority_num();
            if pa != pb {
                pa.cmp(&pb)
            } else {
                a.id.cmp(&b.id)
            }
        });
    }
}

/// Counts of tickets by status
#[derive(Debug, Clone, Copy, Default)]
pub struct StatusCounts {
    pub new: usize,
    pub next: usize,
    pub in_progress: usize,
    pub complete: usize,
    pub cancelled: usize,
}

impl StatusCounts {
    /// Get count for a specific status
    pub fn for_status(&self, status: TicketStatus) -> usize {
        match status {
            TicketStatus::New => self.new,
            TicketStatus::Next => self.next,
            TicketStatus::InProgress => self.in_progress,
            TicketStatus::Complete => self.complete,
            TicketStatus::Cancelled => self.cancelled,
        }
    }

    /// Get total count of all tickets
    pub fn total(&self) -> usize {
        self.new + self.next + self.in_progress + self.complete + self.cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TicketId, TicketPriority, TicketType};

    fn create_test_ticket(
        id: &str,
        status: TicketStatus,
        priority: TicketPriority,
    ) -> TicketMetadata {
        TicketMetadata {
            id: Some(TicketId::new_unchecked(id)),
            status: Some(status),
            priority: Some(priority),
            ticket_type: Some(TicketType::Task),
            ..Default::default()
        }
    }

    #[test]
    fn test_tickets_by_status() {
        let tickets = vec![
            create_test_ticket("j-a1b2", TicketStatus::New, TicketPriority::P2),
            create_test_ticket("j-c3d4", TicketStatus::InProgress, TicketPriority::P1),
            create_test_ticket("j-e5f6", TicketStatus::New, TicketPriority::P3),
        ];

        let new_tickets = TicketAnalytics::tickets_by_status(&tickets, TicketStatus::New);
        assert_eq!(new_tickets.len(), 2);

        let wip_tickets = TicketAnalytics::tickets_by_status(&tickets, TicketStatus::InProgress);
        assert_eq!(wip_tickets.len(), 1);
    }

    #[test]
    fn test_ticket_count() {
        let tickets = vec![
            create_test_ticket("j-a1b2", TicketStatus::New, TicketPriority::P2),
            create_test_ticket("j-c3d4", TicketStatus::InProgress, TicketPriority::P1),
        ];

        assert_eq!(TicketAnalytics::ticket_count(&tickets), 2);
    }

    #[test]
    fn test_status_counts() {
        let tickets = vec![
            create_test_ticket("j-a1b2", TicketStatus::New, TicketPriority::P2),
            create_test_ticket("j-c3d4", TicketStatus::New, TicketPriority::P1),
            create_test_ticket("j-e5f6", TicketStatus::InProgress, TicketPriority::P3),
            create_test_ticket("j-g7h8", TicketStatus::Complete, TicketPriority::P2),
        ];

        let counts = TicketAnalytics::status_counts(&tickets);
        assert_eq!(counts.new, 2);
        assert_eq!(counts.in_progress, 1);
        assert_eq!(counts.complete, 1);
        assert_eq!(counts.next, 0);
        assert_eq!(counts.cancelled, 0);
        assert_eq!(counts.total(), 4);
    }

    #[test]
    fn test_sort_by_priority() {
        let mut tickets = vec![
            create_test_ticket("j-c3d4", TicketStatus::New, TicketPriority::P2),
            create_test_ticket("j-a1b2", TicketStatus::New, TicketPriority::P0),
            create_test_ticket("j-e5f6", TicketStatus::New, TicketPriority::P1),
        ];

        TicketAnalytics::sort_by_priority(&mut tickets);

        assert_eq!(tickets[0].id.as_deref(), Some("j-a1b2")); // P0
        assert_eq!(tickets[1].id.as_deref(), Some("j-e5f6")); // P1
        assert_eq!(tickets[2].id.as_deref(), Some("j-c3d4")); // P2
    }
}
