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
}

impl SearchState {
    /// Create a new search state with the given hooks
    pub fn use_state(hooks: &mut Hooks) -> Self {
        let search_filtered: State<Option<Vec<FilteredTicket>>> = hooks.use_state(|| None);
        let search_in_flight: State<bool> = hooks.use_state(|| false);
        let search_pending = hooks.use_state(|| false);

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

                    let results = if let Some(cache) = crate::cache::get_or_init_cache().await {
                        cache.search_tickets(&query).await.unwrap_or_default()
                    } else {
                        vec![]
                    };

                    let highlighted = compute_title_highlights(&results, &query);
                    search_filtered_setter.set(Some(highlighted));
                    search_in_flight_setter.set(false);
                })
            }
        });

        Self {
            filtered: search_filtered,
            in_flight: search_in_flight,
            pending: search_pending,
            handler: search_handler,
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
        self.handler.clone()(query);
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
            })
            .collect()
    }
}
