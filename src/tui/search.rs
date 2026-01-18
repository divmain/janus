//! Fuzzy search logic for filtering tickets
//!
//! Provides fuzzy matching across multiple ticket fields with support for
//! priority shorthand (p0-p4) filtering.

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use regex::Regex;

use crate::types::TicketMetadata;

/// A ticket with its fuzzy match score and matched indices
#[derive(Debug, Clone, Default)]
pub struct FilteredTicket {
    /// The original ticket metadata
    pub ticket: TicketMetadata,
    /// The fuzzy match score (higher is better)
    pub score: i64,
    /// Indices of matched characters in the title (for highlighting)
    pub title_indices: Vec<usize>,
}

/// Filter tickets by a fuzzy search query
///
/// Supports:
/// - Fuzzy matching across id, title, and type
/// - Priority shorthand: `p0`, `p1`, `p2`, `p3`, `p4` filters by priority
/// - Smart case: case-insensitive unless query contains uppercase
pub fn filter_tickets(tickets: &[TicketMetadata], query: &str) -> Vec<FilteredTicket> {
    if query.is_empty() {
        return tickets
            .iter()
            .map(|t| FilteredTicket {
                ticket: t.clone(),
                score: 0,
                title_indices: vec![],
            })
            .collect();
    }

    let matcher = SkimMatcherV2::default().smart_case();

    // Check for priority shorthand: "p0", "p1", etc.
    let priority_filter = parse_priority_filter(query);

    let mut results: Vec<FilteredTicket> = tickets
        .iter()
        .filter(|t| {
            // Apply priority filter if present
            if let Some(p) = priority_filter
                && t.priority_num() != p
            {
                return false;
            }
            true
        })
        .filter_map(|ticket| {
            // Build searchable text from multiple fields
            let search_text = format!(
                "{} {} {}",
                ticket.id.as_deref().unwrap_or(""),
                ticket.title.as_deref().unwrap_or(""),
                ticket
                    .ticket_type
                    .map(|t| t.to_string())
                    .unwrap_or_default(),
            );

            // Strip priority shorthand from query for fuzzy match
            let fuzzy_query = strip_priority_shorthand(query);

            if fuzzy_query.is_empty() {
                return Some(FilteredTicket {
                    ticket: ticket.clone(),
                    score: 0,
                    title_indices: vec![],
                });
            }

            matcher
                .fuzzy_indices(&search_text, &fuzzy_query)
                .map(|(score, indices)| {
                    // Calculate which indices fall within the title portion
                    let id_len = ticket.id.as_ref().map(|s| s.len()).unwrap_or(0) + 1; // +1 for space
                    let title_len = ticket.title.as_ref().map(|s| s.len()).unwrap_or(0);

                    let title_indices: Vec<usize> = indices
                        .into_iter()
                        .filter(|&i| i >= id_len && i < id_len + title_len)
                        .map(|i| i - id_len)
                        .collect();

                    FilteredTicket {
                        ticket: ticket.clone(),
                        score,
                        title_indices,
                    }
                })
        })
        .collect();

    // Sort by score (best matches first)
    results.sort_by(|a, b| b.score.cmp(&a.score));
    results
}

/// Parse a priority filter from the query (e.g., "p0", "p1", "P2")
fn parse_priority_filter(query: &str) -> Option<u8> {
    let re = Regex::new(r"(?i)\bp([0-4])\b").expect("priority filter regex should be valid");
    re.captures(query)
        .and_then(|c| c.get(1)?.as_str().parse().ok())
}

/// Strip priority shorthand from the query for fuzzy matching
fn strip_priority_shorthand(query: &str) -> String {
    let re = Regex::new(r"(?i)\bp[0-4]\b").expect("priority shorthand regex should be valid");
    re.replace_all(query, "").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TicketPriority, TicketStatus, TicketType};

    fn make_ticket(id: &str, title: &str, priority: u8) -> TicketMetadata {
        TicketMetadata {
            id: Some(id.to_string()),
            title: Some(title.to_string()),
            status: Some(TicketStatus::New),
            priority: Some(match priority {
                0 => TicketPriority::P0,
                1 => TicketPriority::P1,
                2 => TicketPriority::P2,
                3 => TicketPriority::P3,
                _ => TicketPriority::P4,
            }),
            ticket_type: Some(TicketType::Task),
            ..Default::default()
        }
    }

    #[test]
    fn test_empty_query_returns_all() {
        let tickets = vec![
            make_ticket("j-a1b2", "Fix bug", 0),
            make_ticket("j-c3d4", "Add feature", 2),
        ];

        let results = filter_tickets(&tickets, "");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_fuzzy_match_title() {
        let tickets = vec![
            make_ticket("j-a1b2", "Fix bug in parser", 2),
            make_ticket("j-c3d4", "Add new feature", 2),
        ];

        let results = filter_tickets(&tickets, "bug");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ticket.id, Some("j-a1b2".to_string()));
    }

    #[test]
    fn test_priority_filter() {
        let tickets = vec![
            make_ticket("j-a1b2", "Critical fix", 0),
            make_ticket("j-c3d4", "Normal task", 2),
            make_ticket("j-e5f6", "Low priority", 4),
        ];

        let results = filter_tickets(&tickets, "p0");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ticket.id, Some("j-a1b2".to_string()));
    }

    #[test]
    fn test_priority_filter_with_query() {
        let tickets = vec![
            make_ticket("j-a1b2", "Critical fix", 0),
            make_ticket("j-c3d4", "Another critical", 0),
            make_ticket("j-e5f6", "Low priority fix", 4),
        ];

        let results = filter_tickets(&tickets, "p0 fix");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ticket.id, Some("j-a1b2".to_string()));
    }

    #[test]
    fn test_fuzzy_match_id() {
        let tickets = vec![
            make_ticket("j-a1b2", "Fix bug", 2),
            make_ticket("j-c3d4", "Add feature", 2),
        ];

        let results = filter_tickets(&tickets, "a1b2");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ticket.id, Some("j-a1b2".to_string()));
    }

    #[test]
    fn test_parse_priority_filter() {
        assert_eq!(parse_priority_filter("p0"), Some(0));
        assert_eq!(parse_priority_filter("P1"), Some(1));
        assert_eq!(parse_priority_filter("fix p2 bug"), Some(2));
        assert_eq!(parse_priority_filter("no priority"), None);
        assert_eq!(parse_priority_filter("p5"), None); // Invalid priority
    }

    #[test]
    fn test_strip_priority_shorthand() {
        assert_eq!(strip_priority_shorthand("p0"), "");
        assert_eq!(strip_priority_shorthand("p0 fix bug"), "fix bug");
        assert_eq!(strip_priority_shorthand("fix p1 bug"), "fix  bug");
        assert_eq!(strip_priority_shorthand("no priority"), "no priority");
    }
}
