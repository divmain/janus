//! Handler context containing all mutable state references
//!
//! This struct provides a clean interface for handlers to access and modify
//! the board state without needing to pass dozens of individual parameters.

use iocraft::prelude::State;

use crate::tui::action_queue::ActionChannel;
use crate::tui::edit::EditResult;
use crate::tui::edit_state::EditFormState;
use crate::types::TicketMetadata;

use super::super::BoardAction;

/// Context struct holding all mutable state for event handlers
pub struct BoardHandlerContext<'a> {
    pub search_query: &'a mut State<String>,
    pub search_focused: &'a mut State<bool>,
    pub pending_search: &'a mut State<bool>,
    pub should_exit: &'a mut State<bool>,
    pub needs_reload: &'a mut State<bool>,
    pub visible_columns: &'a mut State<[bool; 5]>,
    pub current_column: &'a mut State<usize>,
    pub current_row: &'a mut State<usize>,
    pub column_scroll_offsets: &'a mut State<[usize; 5]>,
    pub column_height: usize,
    pub edit_result: &'a mut State<EditResult>,
    pub is_editing_existing: &'a mut State<bool>,
    pub is_creating_new: &'a mut State<bool>,
    pub editing_ticket: &'a mut State<TicketMetadata>,
    pub editing_body: &'a mut State<String>,
    pub all_tickets: &'a State<Vec<TicketMetadata>>,
    pub action_tx: &'a ActionChannel<BoardAction>,
}

impl<'a> BoardHandlerContext<'a> {
    pub fn edit_state(&mut self) -> EditFormState<'_> {
        EditFormState {
            result: self.edit_result,
            is_editing_existing: self.is_editing_existing,
            is_creating_new: self.is_creating_new,
            editing_ticket: self.editing_ticket,
            editing_body: self.editing_body,
        }
    }
}
