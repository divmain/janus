//! Sort functions for tickets.
//!
//! These functions are used by the query module to sort ticket results.
//! They are re-exported from the display module for backward compatibility.

use crate::types::TicketMetadata;

/// Sort tickets by priority (ascending) then by ID
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

/// Sort tickets by creation date (newest first) then by ID
pub fn sort_by_created(tickets: &mut [TicketMetadata]) {
    tickets.sort_by(|a, b| match (&a.created, &b.created) {
        (Some(date_a), Some(date_b)) => date_b.cmp(date_a),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.id.cmp(&b.id),
    });
}

/// Sort tickets by ID (alphabetical)
pub fn sort_by_id(tickets: &mut [TicketMetadata]) {
    tickets.sort_by(|a, b| a.id.cmp(&b.id));
}

/// Sort tickets by the specified field
pub fn sort_tickets_by(tickets: &mut [TicketMetadata], sort_by: &str) {
    match sort_by {
        "created" => sort_by_created(tickets),
        "id" => sort_by_id(tickets),
        "priority" => sort_by_priority(tickets),
        _ => sort_by_priority(tickets),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TicketMetadata, TicketPriority};

    #[test]
    fn test_sort_by_priority() {
        let mut tickets = vec![
            TicketMetadata {
                id: Some("j-3".to_string()),
                priority: Some(TicketPriority::P3),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-1".to_string()),
                priority: Some(TicketPriority::P0),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-2".to_string()),
                priority: Some(TicketPriority::P1),
                ..Default::default()
            },
        ];

        sort_by_priority(&mut tickets);

        assert_eq!(tickets[0].id, Some("j-1".to_string()));
        assert_eq!(tickets[1].id, Some("j-2".to_string()));
        assert_eq!(tickets[2].id, Some("j-3".to_string()));
    }

    #[test]
    fn test_sort_by_created() {
        let mut tickets = vec![
            TicketMetadata {
                id: Some("j-old".to_string()),
                created: Some("2024-01-01T00:00:00Z".to_string()),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-new".to_string()),
                created: Some("2024-12-01T00:00:00Z".to_string()),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-mid".to_string()),
                created: Some("2024-06-01T00:00:00Z".to_string()),
                ..Default::default()
            },
        ];

        sort_by_created(&mut tickets);

        assert_eq!(tickets[0].id, Some("j-new".to_string()));
        assert_eq!(tickets[1].id, Some("j-mid".to_string()));
        assert_eq!(tickets[2].id, Some("j-old".to_string()));
    }

    #[test]
    fn test_sort_by_id() {
        let mut tickets = vec![
            TicketMetadata {
                id: Some("j-zebra".to_string()),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-alpha".to_string()),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-middle".to_string()),
                ..Default::default()
            },
        ];

        sort_by_id(&mut tickets);

        assert_eq!(tickets[0].id, Some("j-alpha".to_string()));
        assert_eq!(tickets[1].id, Some("j-middle".to_string()));
        assert_eq!(tickets[2].id, Some("j-zebra".to_string()));
    }

    #[test]
    fn test_sort_tickets_by_all_options() {
        let mut tickets1 = vec![
            TicketMetadata {
                id: Some("j-3".to_string()),
                priority: Some(TicketPriority::P3),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-1".to_string()),
                priority: Some(TicketPriority::P0),
                ..Default::default()
            },
        ];
        sort_tickets_by(&mut tickets1, "priority");
        assert_eq!(tickets1[0].id, Some("j-1".to_string()));

        let mut tickets2 = vec![
            TicketMetadata {
                id: Some("j-old".to_string()),
                created: Some("2024-01-01T00:00:00Z".to_string()),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-new".to_string()),
                created: Some("2024-12-01T00:00:00Z".to_string()),
                ..Default::default()
            },
        ];
        sort_tickets_by(&mut tickets2, "created");
        assert_eq!(tickets2[0].id, Some("j-new".to_string()));

        let mut tickets3 = vec![
            TicketMetadata {
                id: Some("j-zebra".to_string()),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-alpha".to_string()),
                ..Default::default()
            },
        ];
        sort_tickets_by(&mut tickets3, "id");
        assert_eq!(tickets3[0].id, Some("j-alpha".to_string()));

        let mut tickets4 = vec![
            TicketMetadata {
                id: Some("j-3".to_string()),
                priority: Some(TicketPriority::P3),
                ..Default::default()
            },
            TicketMetadata {
                id: Some("j-1".to_string()),
                priority: Some(TicketPriority::P0),
                ..Default::default()
            },
        ];
        sort_tickets_by(&mut tickets4, "invalid_option");
        assert_eq!(tickets4[0].id, Some("j-1".to_string()));
    }
}
