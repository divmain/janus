//! Fuzzy search logic for filtering tickets
//!
//! Provides fuzzy matching across multiple ticket fields with support for
//! priority shorthand (p0-p4) filtering.

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use regex::Regex;
use std::sync::Arc;

use crate::types::TicketMetadata;

/// A ticket with its fuzzy match score and matched indices
#[derive(Debug, Clone, Default)]
pub struct FilteredTicket {
    /// The original ticket metadata (shared via Arc to avoid cloning)
    pub ticket: Arc<TicketMetadata>,
    /// The fuzzy match score (higher is better)
    pub score: i64,
    /// Indices of matched characters in the title (for highlighting)
    pub title_indices: Vec<usize>,
}

/// Generic result of fuzzy filtering with item, score, and title indices
#[derive(Debug, Clone)]
pub struct FilteredItem<T> {
    /// The filtered item
    pub item: T,
    /// The fuzzy match score (higher is better)
    pub score: i64,
    /// Indices of matched characters in the title (for highlighting)
    pub title_indices: Vec<usize>,
}

/// Generic fuzzy filter function that can work with any item type
///
/// # Parameters
/// - `items`: Slice of items to filter
/// - `query`: Search query string
/// - `make_searchable`: Function to convert an item to searchable text
/// - `extract_title_info`: Function to extract (id_len, title_len) for title highlighting
///
/// # Returns
/// Vector of `FilteredItem<T>` sorted by score (best matches first)
pub fn filter_items<T, F, G>(
    items: &[T],
    query: &str,
    make_searchable: F,
    extract_title_info: G,
) -> Vec<FilteredItem<T>>
where
    T: Clone,
    F: Fn(&T) -> String,
    G: Fn(&T) -> (usize, usize),
{
    if query.is_empty() {
        return items
            .iter()
            .map(|item| FilteredItem {
                item: item.clone(),
                score: 0,
                title_indices: vec![],
            })
            .collect();
    }

    let matcher = SkimMatcherV2::default().smart_case();

    let mut results: Vec<FilteredItem<T>> = items
        .iter()
        .filter_map(|item| {
            let search_text = make_searchable(item);

            matcher
                .fuzzy_indices(&search_text, query)
                .map(|(score, indices)| {
                    let (id_len, title_len) = extract_title_info(item);

                    let title_indices: Vec<usize> = indices
                        .into_iter()
                        .filter(|&i| i >= id_len && i < id_len + title_len)
                        .map(|i| i - id_len)
                        .collect();

                    FilteredItem {
                        item: item.clone(),
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
                ticket: Arc::new(t.clone()),
                score: 0,
                title_indices: vec![],
            })
            .collect();
    }

    // Check for priority shorthand: "p0", "p1", etc.
    let priority_filter = parse_priority_filter(query);

    // Pre-filter by priority if needed
    let filtered_tickets: Vec<&TicketMetadata> = if let Some(p) = priority_filter {
        tickets.iter().filter(|t| t.priority_num() == p).collect()
    } else {
        tickets.iter().collect()
    };

    // Strip priority shorthand from query for fuzzy match
    let fuzzy_query = strip_priority_shorthand(query);

    if fuzzy_query.is_empty() {
        return filtered_tickets
            .iter()
            .map(|t| FilteredTicket {
                ticket: Arc::new((*t).clone()),
                score: 0,
                title_indices: vec![],
            })
            .collect();
    }

    // Use generic filter function
    let results = filter_items(
        &filtered_tickets,
        &fuzzy_query,
        |ticket| {
            format!(
                "{} {} {}",
                ticket.id.as_deref().unwrap_or(""),
                ticket.title.as_deref().unwrap_or(""),
                ticket
                    .ticket_type
                    .map(|t| t.to_string())
                    .unwrap_or_default(),
            )
        },
        |ticket| {
            let id_len = ticket.id.as_ref().map(|s| s.len()).unwrap_or(0) + 1; // +1 for space
            let title_len = ticket.title.as_ref().map(|s| s.len()).unwrap_or(0);
            (id_len, title_len)
        },
    );

    // Convert from FilteredItem<&TicketMetadata> to FilteredTicket
    results
        .into_iter()
        .map(|filtered| FilteredTicket {
            ticket: Arc::new((*filtered.item).clone()),
            score: filtered.score,
            title_indices: filtered.title_indices,
        })
        .collect()
}

/// Parse a priority filter from the query (e.g., "p0", "p1", "P2")
pub fn parse_priority_filter(query: &str) -> Option<u8> {
    let re = Regex::new(r"(?i)\bp([0-4])\b").expect("priority filter regex should be valid");
    re.captures(query)
        .and_then(|c| c.get(1)?.as_str().parse().ok())
}

/// Strip priority shorthand from the query for fuzzy matching
pub fn strip_priority_shorthand(query: &str) -> String {
    let re = Regex::new(r"(?i)\bp[0-4]\b").expect("priority shorthand regex should be valid");
    re.replace_all(query, "").trim().to_string()
}

/// Calculate search debounce delay based on ticket count.
///
/// Larger repositories get longer debounce to avoid excessive queries
/// while typing. Smaller repos get near-instant search.
pub fn calculate_debounce_ms(ticket_count: usize) -> u64 {
    match ticket_count {
        0..=100 => 10,
        101..=500 => 50,
        501..=1000 => 100,
        _ => 150,
    }
}

/// Compute title highlight indices for tickets returned from SQL search.
///
/// Runs lightweight fuzzy matching on title only (not body) to determine
/// which characters to highlight in the UI.
pub fn compute_title_highlights(tickets: &[TicketMetadata], query: &str) -> Vec<FilteredTicket> {
    let text_query = strip_priority_shorthand(query);
    let text_query = text_query.trim();

    if text_query.is_empty() {
        // No text query, no highlighting needed
        return tickets
            .iter()
            .map(|t| FilteredTicket {
                ticket: Arc::new(t.clone()),
                score: 0,
                title_indices: vec![],
            })
            .collect();
    }

    let matcher = SkimMatcherV2::default().smart_case();

    tickets
        .iter()
        .map(|ticket| {
            let title = ticket.title.as_deref().unwrap_or("");
            let title_indices = matcher
                .fuzzy_indices(title, text_query)
                .map(|(_, indices)| indices)
                .unwrap_or_default();

            FilteredTicket {
                ticket: Arc::new(ticket.clone()),
                score: 0, // Score not relevant for SQL-based search
                title_indices,
            }
        })
        .collect()
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

    #[test]
    fn test_calculate_debounce_ms() {
        assert_eq!(calculate_debounce_ms(0), 10);
        assert_eq!(calculate_debounce_ms(50), 10);
        assert_eq!(calculate_debounce_ms(100), 10);
        assert_eq!(calculate_debounce_ms(101), 50);
        assert_eq!(calculate_debounce_ms(500), 50);
        assert_eq!(calculate_debounce_ms(501), 100);
        assert_eq!(calculate_debounce_ms(1000), 100);
        assert_eq!(calculate_debounce_ms(1001), 150);
        assert_eq!(calculate_debounce_ms(10000), 150);
    }

    #[test]
    fn test_compute_title_highlights_empty_query() {
        let tickets = vec![
            make_ticket("j-a1b2", "Fix authentication bug", 0),
            make_ticket("j-c3d4", "Add new feature", 2),
        ];

        let results = compute_title_highlights(&tickets, "");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title_indices, Vec::<usize>::new());
        assert_eq!(results[1].title_indices, Vec::<usize>::new());
    }

    #[test]
    fn test_compute_title_highlights_with_match() {
        let tickets = vec![
            make_ticket("j-a1b2", "Fix authentication", 0),
            make_ticket("j-c3d4", "Add new feature", 2),
            make_ticket("j-e5f6", "Update authentication module", 0),
        ];

        let results = compute_title_highlights(&tickets, "auth");
        assert_eq!(results.len(), 3);
        // First ticket: "Fix authentication" - should highlight positions 4-8
        assert!(!results[0].title_indices.is_empty());
        // Second ticket: no match - should have empty indices
        assert!(results[1].title_indices.is_empty());
        // Third ticket: should have highlights
        assert!(!results[2].title_indices.is_empty());
    }

    #[test]
    fn test_compute_title_highlights_priority_shorthand() {
        let tickets = vec![
            make_ticket("j-a1b2", "Fix critical bug", 0),
            make_ticket("j-c3d4", "Add feature", 2),
        ];

        // Priority-only query, no text matching
        let results = compute_title_highlights(&tickets, "p0");
        assert_eq!(results.len(), 2);
        assert!(results[0].title_indices.is_empty());
        assert!(results[1].title_indices.is_empty());

        // Priority with text - should strip priority and match on text
        let results = compute_title_highlights(&tickets, "p0 bug");
        assert_eq!(results.len(), 2);
        // First ticket should match "bug"
        assert!(!results[0].title_indices.is_empty());
        // Second ticket has no "bug"
        assert!(results[1].title_indices.is_empty());
    }
}
