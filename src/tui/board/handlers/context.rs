//! Handler context containing all mutable state references
//!
//! This struct provides a clean interface for handlers to access and modify
//! the board state without needing to pass dozens of individual parameters.

use iocraft::prelude::{Handler, State};

use crate::tui::edit::EditResult;
use crate::tui::edit_state::{EditFormState, EditMode};
use crate::tui::search_orchestrator::SearchState as SearchOrchestrator;
use crate::types::{TicketMetadata, TicketStatus};

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
}

impl<'a> BoardHandlerContext<'a> {
    pub fn edit_state(&mut self) -> EditFormState<'_> {
        EditFormState {
            mode: self.edit_mode,
            result: self.edit_result,
        }
    }
}
