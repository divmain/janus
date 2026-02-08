//! Fuzzy and semantic search logic for filtering tickets
//!
//! Provides two search modes:
//! - Fuzzy matching (default): Uses substring/fuzzy matching across ticket fields
//! - Semantic matching (with ~ prefix): Uses vector embeddings for intent-based search
//!
//! To use semantic search, prefix your query with `~`:
//! - `authentication` - Fuzzy search for "authentication"
//! - `~authentication` - Semantic search for tickets related to authentication
//!
//! Features:
//! - Fuzzy matching across id, title, and type
//! - Priority shorthand: `p0`, `p1`, `p2`, `p3`, `p4`
//! - Smart case: case-insensitive unless query contains uppercase
//! - Semantic search with `~` prefix
//! - Result merging: Fuzzy results first, then semantic (deduplicated)

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use std::sync::Arc;

// Re-export priority filter utilities from shared utils module
pub use crate::utils::{parse_priority_filter, strip_priority_shorthand};

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
    /// true if from semantic search
    pub is_semantic: bool,
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
                is_semantic: false,
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
                is_semantic: false,
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
            is_semantic: false,
        })
        .collect()
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

/// Check if query uses semantic search modifier (~ prefix)
pub fn is_semantic_search(query: &str) -> bool {
    query.starts_with('~')
}

/// Strip the semantic modifier and return clean query
pub fn strip_semantic_modifier(query: &str) -> &str {
    if let Some(stripped) = query.strip_prefix('~') {
        stripped.trim_start()
    } else {
        query
    }
}

/// Perform semantic search using the in-memory store.
/// Returns SearchResults on success, or an error string on failure.
pub async fn perform_semantic_search(
    query: &str,
) -> std::result::Result<Vec<crate::store::search::SearchResult>, String> {
    use crate::remote::config::Config;
    use crate::store::get_or_init_store;

    // Check if semantic search is enabled
    match Config::load() {
        Ok(config) => {
            if !config.semantic_search_enabled() {
                return Err("Semantic search is disabled. Enable with: janus config set semantic_search.enabled true".to_string());
            }
        }
        Err(e) => {
            eprintln!(
                "Warning: failed to load config: {e}. Proceeding with semantic search."
            );
        }
    }

    // Get store
    let store = get_or_init_store()
        .await
        .map_err(|e| format!("Failed to initialize store: {e}"))?;

    // Check if embeddings are available
    let (with_embedding, total) = store.embedding_coverage();

    if total == 0 {
        return Err("No tickets available".to_string());
    }

    if with_embedding == 0 {
        return Err("No ticket embeddings available. Run 'janus cache rebuild' to generate embeddings for all tickets.".to_string());
    }

    // Generate query embedding and perform semantic search
    let query_embedding = crate::embedding::model::generate_embedding(query)
        .await
        .map_err(|e| format!("Failed to generate query embedding: {e}"))?;
    let results = store.semantic_search(&query_embedding, 10);

    Ok(results)
}

/// Merge fuzzy and semantic results, removing duplicates
/// Fuzzy results take precedence (appear first)
pub fn merge_search_results(
    fuzzy: Vec<FilteredTicket>,
    semantic: Vec<crate::store::search::SearchResult>,
) -> Vec<FilteredTicket> {
    use std::collections::HashSet;

    // Collect IDs from fuzzy results to avoid duplicates
    let fuzzy_ids: HashSet<String> = fuzzy.iter().filter_map(|t| t.ticket.id.clone()).collect();

    // Convert semantic results to FilteredTickets, excluding duplicates
    let semantic_tickets: Vec<FilteredTicket> = semantic
        .into_iter()
        .filter(|r| {
            r.ticket
                .id
                .as_ref()
                .map(|id| !fuzzy_ids.contains(id))
                .unwrap_or(false)
        })
        .map(|r| r.into())
        .collect();

    // Combine: fuzzy first, then semantic
    let mut result = fuzzy;
    result.extend(semantic_tickets);
    result
}

impl From<crate::store::search::SearchResult> for FilteredTicket {
    fn from(result: crate::store::search::SearchResult) -> Self {
        Self {
            ticket: Arc::new(result.ticket),
            score: 0,              // Fuzzy score not applicable
            title_indices: vec![], // No fuzzy highlighting
            is_semantic: true,
        }
    }
}

/// Compute title highlight indices for tickets returned from text search.
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
                is_semantic: false,
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
                score: 0, // Score not relevant for store-based search
                title_indices,
                is_semantic: false,
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

    #[test]
    fn test_is_semantic_search() {
        assert!(is_semantic_search("~query"));
        assert!(is_semantic_search("~ query"));
        assert!(!is_semantic_search("query"));
        assert!(!is_semantic_search("query~"));
        assert!(!is_semantic_search(""));
    }

    #[test]
    fn test_strip_semantic_modifier() {
        assert_eq!(strip_semantic_modifier("~query"), "query");
        assert_eq!(strip_semantic_modifier("~ query"), "query");
        assert_eq!(strip_semantic_modifier("query"), "query");
        assert_eq!(strip_semantic_modifier("~"), "");
    }

    #[test]
    fn test_merge_search_results() {
        // Create fuzzy results
        let fuzzy = vec![FilteredTicket {
            ticket: Arc::new(TicketMetadata {
                id: Some("ticket-1".to_string()),
                title: Some("First Ticket".to_string()),
                ..Default::default()
            }),
            score: 100,
            title_indices: vec![],
            is_semantic: false,
        }];

        // Create semantic results (including duplicate)
        let semantic = vec![
            crate::store::search::SearchResult {
                ticket: TicketMetadata {
                    id: Some("ticket-1".to_string()), // Duplicate
                    title: Some("First Ticket".to_string()),
                    ..Default::default()
                },
                similarity: 0.95,
            },
            crate::store::search::SearchResult {
                ticket: TicketMetadata {
                    id: Some("ticket-2".to_string()), // New
                    title: Some("Second Ticket".to_string()),
                    ..Default::default()
                },
                similarity: 0.85,
            },
        ];

        let merged = merge_search_results(fuzzy, semantic);

        // Should have 2 results (ticket-1 from fuzzy, ticket-2 from semantic)
        assert_eq!(merged.len(), 2);

        // First should be fuzzy (ticket-1)
        assert_eq!(merged[0].ticket.id.as_ref().unwrap(), "ticket-1");
        assert!(!merged[0].is_semantic);

        // Second should be semantic (ticket-2)
        assert_eq!(merged[1].ticket.id.as_ref().unwrap(), "ticket-2");
        assert!(merged[1].is_semantic);
    }

    #[test]
    fn test_is_semantic_search_edge_cases() {
        // Empty query with ~ prefix
        assert!(is_semantic_search("~"));
        assert!(is_semantic_search("~ "));

        // Very long query
        assert!(is_semantic_search(
            "~a very long query with many words to test edge cases"
        ));

        // Special characters after ~
        assert!(is_semantic_search("~!@#$%^&*()"));
        assert!(is_semantic_search("~query-with-dashes"));
        assert!(is_semantic_search("~query_with_underscores"));

        // Multiple ~ characters
        assert!(is_semantic_search("~~query"));
        assert!(is_semantic_search("~query~more"));
        assert!(is_semantic_search("~~~"));
    }

    #[test]
    fn test_strip_semantic_modifier_edge_cases() {
        // Only ~
        assert_eq!(strip_semantic_modifier("~"), "");

        // ~ followed by spaces
        assert_eq!(strip_semantic_modifier("~   "), "");
        assert_eq!(strip_semantic_modifier("~  query"), "query");

        // Without ~ prefix (no change)
        assert_eq!(
            strip_semantic_modifier("query without prefix"),
            "query without prefix"
        );
        assert_eq!(strip_semantic_modifier(""), "");

        // Multiple ~ (only strips first)
        assert_eq!(strip_semantic_modifier("~~query"), "~query");
        assert_eq!(strip_semantic_modifier("~query~more"), "query~more");
    }
}
