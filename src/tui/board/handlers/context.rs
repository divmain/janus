//! Handler context containing all mutable state references
//!
//! This struct provides a clean interface for handlers to access and modify
//! the board state without needing to pass dozens of individual parameters.

use iocraft::prelude::State;
use tokio::sync::mpsc;

use crate::tui::edit::EditResult;
use crate::tui::edit_state::EditFormState;
use crate::types::TicketMetadata;

use super::types::TicketAction;

/// Context struct holding all mutable state for event handlers
pub struct BoardHandlerContext<'a> {
    // Search state
    pub search_query: &'a mut State<String>,
    pub search_focused: &'a mut State<bool>,

    // App state
    pub should_exit: &'a mut State<bool>,
    pub needs_reload: &'a mut State<bool>,

    // Column visibility state
    pub visible_columns: &'a mut State<[bool; 5]>,

    // Navigation state
    pub current_column: &'a mut State<usize>,
    pub current_row: &'a mut State<usize>,

    // Edit form state
    pub edit_result: &'a mut State<EditResult>,
    pub is_editing_existing: &'a mut State<bool>,
    pub is_creating_new: &'a mut State<bool>,
    pub editing_ticket: &'a mut State<TicketMetadata>,
    pub editing_body: &'a mut State<String>,

    // Data (read-only reference for operations)
    pub all_tickets: &'a State<Vec<TicketMetadata>>,

    // Async action queue sender
    pub action_tx: &'a mpsc::UnboundedSender<TicketAction>,
}

impl<'a> BoardHandlerContext<'a> {
    /// Create an EditFormState from the context's edit-related fields
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
