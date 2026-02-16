//! Sort functions for tickets.
//!
//! These functions are used by the query module to sort ticket results.
//! They are re-exported from the display module for backward compatibility.

use crate::types::{TicketData, TicketMetadata};

/// Sort field for ticket listing and queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortField {
    Created,
    Id,
    #[default]
    Priority,
}

impl SortField {
    /// All valid string representations of this enum.
    pub const ALL_STRINGS: &[&str] = &["priority", "created", "id"];
}

enum_display_fromstr!(
    SortField,
    crate::error::JanusError::invalid_sort_field,
    ["priority", "created", "id"],
    {
        Created => "created",
        Id => "id",
        Priority => "priority",
    }
);

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
pub fn sort_tickets_by(tickets: &mut [TicketMetadata], sort_by: SortField) {
    match sort_by {
        SortField::Created => sort_by_created(tickets),
        SortField::Id => sort_by_id(tickets),
        SortField::Priority => sort_by_priority(tickets),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::types::{CreatedAt, TicketId};
    use crate::types::{TicketMetadata, TicketPriority};

    #[test]
    fn test_sort_by_priority() {
        let mut tickets = vec![
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-3")),
                priority: Some(TicketPriority::P3),
                ..Default::default()
            },
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-1")),
                priority: Some(TicketPriority::P0),
                ..Default::default()
            },
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-2")),
                priority: Some(TicketPriority::P1),
                ..Default::default()
            },
        ];

        sort_by_priority(&mut tickets);

        assert_eq!(tickets[0].id.as_deref(), Some("j-1"));
        assert_eq!(tickets[1].id.as_deref(), Some("j-2"));
        assert_eq!(tickets[2].id.as_deref(), Some("j-3"));
    }

    #[test]
    fn test_sort_by_created() {
        let mut tickets = vec![
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-old")),
                created: Some(CreatedAt::new_unchecked("2024-01-01T00:00:00Z")),
                ..Default::default()
            },
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-new")),
                created: Some(CreatedAt::new_unchecked("2024-12-01T00:00:00Z")),
                ..Default::default()
            },
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-mid")),
                created: Some(CreatedAt::new_unchecked("2024-06-01T00:00:00Z")),
                ..Default::default()
            },
        ];

        sort_by_created(&mut tickets);

        assert_eq!(tickets[0].id.as_deref(), Some("j-new"));
        assert_eq!(tickets[1].id.as_deref(), Some("j-mid"));
        assert_eq!(tickets[2].id.as_deref(), Some("j-old"));
    }

    #[test]
    fn test_sort_by_id() {
        let mut tickets = vec![
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-zebra")),
                ..Default::default()
            },
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-alpha")),
                ..Default::default()
            },
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-middle")),
                ..Default::default()
            },
        ];

        sort_by_id(&mut tickets);

        assert_eq!(tickets[0].id.as_deref(), Some("j-alpha"));
        assert_eq!(tickets[1].id.as_deref(), Some("j-middle"));
        assert_eq!(tickets[2].id.as_deref(), Some("j-zebra"));
    }

    #[test]
    fn test_sort_tickets_by_all_options() {
        let mut tickets1 = vec![
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-3")),
                priority: Some(TicketPriority::P3),
                ..Default::default()
            },
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-1")),
                priority: Some(TicketPriority::P0),
                ..Default::default()
            },
        ];
        sort_tickets_by(&mut tickets1, SortField::Priority);
        assert_eq!(tickets1[0].id.as_deref(), Some("j-1"));

        let mut tickets2 = vec![
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-old")),
                created: Some(CreatedAt::new_unchecked("2024-01-01T00:00:00Z")),
                ..Default::default()
            },
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-new")),
                created: Some(CreatedAt::new_unchecked("2024-12-01T00:00:00Z")),
                ..Default::default()
            },
        ];
        sort_tickets_by(&mut tickets2, SortField::Created);
        assert_eq!(tickets2[0].id.as_deref(), Some("j-new"));

        let mut tickets3 = vec![
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-zebra")),
                ..Default::default()
            },
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-alpha")),
                ..Default::default()
            },
        ];
        sort_tickets_by(&mut tickets3, SortField::Id);
        assert_eq!(tickets3[0].id.as_deref(), Some("j-alpha"));
    }

    #[test]
    fn test_sort_field_from_str() {
        assert_eq!(
            SortField::from_str("priority").unwrap(),
            SortField::Priority
        );
        assert_eq!(SortField::from_str("created").unwrap(), SortField::Created);
        assert_eq!(SortField::from_str("id").unwrap(), SortField::Id);

        // Case insensitive
        assert_eq!(
            SortField::from_str("PRIORITY").unwrap(),
            SortField::Priority
        );
        assert_eq!(SortField::from_str("Created").unwrap(), SortField::Created);
        assert_eq!(SortField::from_str("ID").unwrap(), SortField::Id);
    }

    #[test]
    fn test_sort_field_from_str_invalid() {
        assert!(SortField::from_str("invalid").is_err());
        assert!(SortField::from_str("priorty").is_err());
        assert!(SortField::from_str("").is_err());
        assert!(SortField::from_str("date").is_err());
    }

    #[test]
    fn test_sort_field_display() {
        assert_eq!(SortField::Priority.to_string(), "priority");
        assert_eq!(SortField::Created.to_string(), "created");
        assert_eq!(SortField::Id.to_string(), "id");
    }

    #[test]
    fn test_sort_field_default() {
        assert_eq!(SortField::default(), SortField::Priority);
    }
}
