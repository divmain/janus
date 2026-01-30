//! Shared search orchestration for TUI views
//!
//! This module provides reusable components for handling search state
//! across different views (browser, board, etc.) with consistent behavior:
//! - Search is triggered (not while typing)
//! - SQL search via cache
//! - Search in-flight indicator
//! - Result caching

use iocraft::prelude::*;
use std::sync::Arc;

use crate::tui::search::{FilteredTicket, compute_title_highlights};
use crate::types::TicketMetadata;

/// State for search functionality in TUI views
pub struct SearchState {
    /// Filtered tickets from search (with title highlights)
    pub filtered: State<Option<Vec<FilteredTicket>>>,
    /// Track if search is currently running (for loading indicator)
    pub in_flight: State<bool>,
    /// Track if search needs to be triggered (set by Enter key handler)
    pub pending: State<bool>,
    /// The search handler to execute searches
    pub handler: Handler<String>,
    /// Track if semantic search is running
    pub semantic_pending: State<bool>,
    /// Store semantic search error for toast display
    pub semantic_error: State<Option<String>>,
    /// Handler for semantic search (feature-gated)
    #[cfg(feature = "semantic-search")]
    pub semantic_handler: Handler<String>,
}

impl SearchState {
    /// Create a new search state with the given hooks
    pub fn use_state(hooks: &mut Hooks) -> Self {
        let search_filtered: State<Option<Vec<FilteredTicket>>> = hooks.use_state(|| None);
        let search_in_flight: State<bool> = hooks.use_state(|| false);
        let search_pending = hooks.use_state(|| false);
        let semantic_pending = hooks.use_state(|| false);
        let semantic_error: State<Option<String>> = hooks.use_state(|| None);

        // Fuzzy search handler
        let search_handler: Handler<String> = hooks.use_async_handler({
            let search_filtered_setter = search_filtered;
            let search_in_flight_setter = search_in_flight;

            move |query: String| {
                let mut search_filtered_setter = search_filtered_setter;
                let mut search_in_flight_setter = search_in_flight_setter;

                Box::pin(async move {
                    if query.is_empty() {
                        search_filtered_setter.set(None);
                        search_in_flight_setter.set(false);
                        return;
                    }

                    // Strip ~ prefix for fuzzy search if present
                    let fuzzy_query = if let Some(stripped) = query.strip_prefix('~') {
                        stripped.trim_start()
                    } else {
                        &query
                    };

                    let results = if let Some(cache) = crate::cache::get_or_init_cache().await {
                        cache.search_tickets(fuzzy_query).await.unwrap_or_default()
                    } else {
                        vec![]
                    };

                    let highlighted = compute_title_highlights(&results, fuzzy_query);
                    search_filtered_setter.set(Some(highlighted));
                    search_in_flight_setter.set(false);
                })
            }
        });

        // Semantic search handler (only with feature flag)
        #[cfg(feature = "semantic-search")]
        let semantic_handler: Handler<String> = hooks.use_async_handler({
            let semantic_filtered_setter = search_filtered;
            let semantic_pending_setter = semantic_pending;
            let semantic_error_setter = semantic_error;

            move |query: String| {
                let mut semantic_filtered_setter = semantic_filtered_setter;
                let mut semantic_pending_setter = semantic_pending_setter;
                let mut semantic_error_setter = semantic_error_setter;

                Box::pin(async move {
                    // Only run if query starts with ~
                    let Some(clean_query) = query.strip_prefix('~') else {
                        return;
                    };
                    let clean_query = clean_query.trim_start();

                    // Perform semantic search
                    match crate::tui::search::perform_semantic_search(clean_query).await {
                        Ok(semantic_results) => {
                            // Get current fuzzy results and merge
                            let current_filtered = semantic_filtered_setter.read().clone();
                            if let Some(fuzzy_results) = current_filtered {
                                let merged = crate::tui::search::merge_search_results(
                                    fuzzy_results,
                                    semantic_results,
                                );
                                semantic_filtered_setter.set(Some(merged));
                            } else {
                                // Convert semantic results to FilteredTickets
                                let semantic_tickets: Vec<FilteredTicket> =
                                    semantic_results.into_iter().map(|r| r.into()).collect();
                                semantic_filtered_setter.set(Some(semantic_tickets));
                            }
                            semantic_error_setter.set(None);
                        }
                        Err(e) => {
                            semantic_error_setter.set(Some(e));
                        }
                    }

                    semantic_pending_setter.set(false);
                })
            }
        });

        Self {
            filtered: search_filtered,
            in_flight: search_in_flight,
            pending: search_pending,
            handler: search_handler,
            semantic_pending,
            semantic_error,
            #[cfg(feature = "semantic-search")]
            semantic_handler,
        }
    }

    /// Set the pending flag to trigger search on next render
    pub fn trigger_pending(&mut self) {
        self.pending.set(true);
    }

    /// Trigger search with the given query string
    pub fn trigger(&mut self, query: String) {
        self.pending.set(false);
        self.in_flight.set(true);

        // Always trigger fuzzy search
        self.handler.clone()(query.clone());

        // Trigger semantic if query starts with ~
        #[cfg(feature = "semantic-search")]
        if query.starts_with('~') {
            self.semantic_pending.set(true);
            self.semantic_handler.clone()(query);
        }
    }

    /// Check if search is pending and trigger it
    pub fn check_pending(&mut self, query: String) {
        if self.pending.get() {
            self.trigger(query);
        }
    }

    /// Clear search results if query is empty
    pub fn clear_if_empty(&mut self, query: &str) {
        if query.is_empty() && self.filtered.read().is_some() {
            self.filtered.set(None);
        }
    }

    /// Get filtered tickets for display
    pub fn get_results(&self) -> Option<Vec<FilteredTicket>> {
        self.filtered.read().clone()
    }

    /// Check if search is currently in flight
    pub fn is_in_flight(&self) -> bool {
        self.in_flight.get()
    }

    /// Check if there's a semantic search error to display
    pub fn take_semantic_error(&mut self) -> Option<String> {
        let error = self.semantic_error.read().clone();
        if error.is_some() {
            self.semantic_error.set(None);
        }
        error
    }

    /// Check if semantic search is currently running
    pub fn is_semantic_in_flight(&self) -> bool {
        self.semantic_pending.get()
    }
}

/// Compute which tickets to display based on search state
///
/// # Arguments
/// * `all_tickets` - All tickets in the system
/// * `search_state` - Search state with cached results
/// * `query` - Current search query
///
/// # Returns
/// Filtered tickets to display (with highlights if search results)
pub fn compute_filtered_tickets(
    all_tickets: &[TicketMetadata],
    search_state: &SearchState,
    query: &str,
) -> Vec<FilteredTicket> {
    if query.is_empty() {
        all_tickets
            .iter()
            .map(|t: &TicketMetadata| FilteredTicket {
                ticket: Arc::new(t.clone()),
                score: 0,
                title_indices: vec![],
                is_semantic: false,
            })
            .collect()
    } else if let Some(results) = search_state.get_results() {
        results
    } else {
        all_tickets
            .iter()
            .map(|t: &TicketMetadata| FilteredTicket {
                ticket: Arc::new(t.clone()),
                score: 0,
                title_indices: vec![],
                is_semantic: false,
            })
            .collect()
    }
}
