//! Handler context containing all mutable state references
//!
//! This struct provides a clean interface for handlers to access and modify
//! the board state without needing to pass dozens of individual parameters.

use iocraft::prelude::{Handler, State};

use crate::tui::edit::EditResult;
use crate::tui::edit_state::{EditFormState, EditMode};
use crate::tui::search::{FilteredTicket, filter_tickets};
use crate::tui::search_orchestrator::SearchState as SearchOrchestrator;
use crate::types::{TicketMetadata, TicketStatus};

/// The 5 kanban columns in order
const COLUMNS: [TicketStatus; 5] = [
    TicketStatus::New,
    TicketStatus::Next,
    TicketStatus::InProgress,
    TicketStatus::Complete,
    TicketStatus::Cancelled,
];

/// Cached filtered tickets grouped by column
#[derive(Clone)]
pub struct FilteredCache {
    /// The search query that was used to compute this cache
    query: String,
    /// Filtered tickets grouped by column index
    column_tickets: Vec<Vec<FilteredTicket>>,
}

/// Async handlers for board operations
pub struct BoardAsyncHandlers<'a> {
    pub update_status: &'a Handler<(String, TicketStatus)>,
}

/// Context struct holding all mutable state for event handlers
pub struct BoardHandlerContext<'a> {
    pub search_query: &'a mut State<String>,
    pub search_focused: &'a mut State<bool>,
    pub search_orchestrator: &'a mut SearchOrchestrator,
    pub should_exit: &'a mut State<bool>,
    pub needs_reload: &'a mut State<bool>,
    pub visible_columns: &'a mut State<[bool; 5]>,
    pub current_column: &'a mut State<usize>,
    pub current_row: &'a mut State<usize>,
    pub column_scroll_offsets: &'a mut State<[usize; 5]>,
    pub column_height: usize,
    pub edit_mode: &'a mut State<EditMode>,
    pub edit_result: &'a mut State<EditResult>,
    pub all_tickets: &'a State<Vec<TicketMetadata>>,
    pub handlers: BoardAsyncHandlers<'a>,
    /// Cached filtered tickets to avoid repeated filtering on every keypress
    pub cache: &'a mut State<Option<FilteredCache>>,
}

impl<'a> BoardHandlerContext<'a> {
    pub fn edit_state(&mut self) -> EditFormState<'_> {
        EditFormState {
            mode: self.edit_mode,
            result: self.edit_result,
        }
    }

    /// Get the count of tickets in a specific column, using cache if available
    pub fn get_column_count(&mut self, column: usize) -> usize {
        if column >= COLUMNS.len() {
            return 0;
        }
        self.get_cached_column_tickets(column).len()
    }

    /// Get the ticket at a specific column and row, using cache
    pub fn get_ticket_at(&mut self, column: usize, row: usize) -> Option<TicketMetadata> {
        if column >= COLUMNS.len() {
            return None;
        }
        let column_tickets = self.get_cached_column_tickets(column);
        column_tickets.get(row).map(|ft| ft.ticket.as_ref().clone())
    }

    /// Get cached filtered tickets for a column, computing if necessary
    fn get_cached_column_tickets(&mut self, column: usize) -> Vec<FilteredTicket> {
        let current_query = self.search_query.to_string();

        // Check if cache is valid
        let cache_valid = self
            .cache
            .read()
            .as_ref()
            .map(|c| c.query == current_query)
            .unwrap_or(false);

        if !cache_valid {
            // Use orchestrator results if available (includes merged fuzzy + semantic results)
            // Otherwise fall back to filtering directly
            let filtered = if let Some(results) = self.search_orchestrator.get_results() {
                results
            } else if current_query.is_empty() {
                self.all_tickets
                    .read()
                    .iter()
                    .map(|t| FilteredTicket {
                        ticket: std::sync::Arc::new(t.clone()),
                        score: 0,
                        title_indices: vec![],
                        is_semantic: false,
                    })
                    .collect()
            } else {
                let tickets_read = self.all_tickets.read();
                filter_tickets(&tickets_read, &current_query)
            };

            let column_tickets: Vec<Vec<FilteredTicket>> = COLUMNS
                .iter()
                .map(|status| {
                    filtered
                        .iter()
                        .filter(|ft| ft.ticket.status.unwrap_or_default() == *status)
                        .cloned()
                        .collect()
                })
                .collect();

            self.cache.set(Some(FilteredCache {
                query: current_query,
                column_tickets,
            }));
        }

        // Return the cached column tickets
        self.cache
            .read()
            .as_ref()
            .map(|c| c.column_tickets[column].clone())
            .unwrap_or_default()
    }
}
