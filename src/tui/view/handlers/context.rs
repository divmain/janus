//! Handler context containing all mutable state references
//!
//! This struct provides a clean interface for handlers to access and modify
//! the TUI state without needing to pass dozens of individual parameters.

use iocraft::prelude::State;

use crate::tui::action_queue::ActionChannel;
use crate::tui::edit::EditResult;
use crate::tui::edit_state::EditFormState;
use crate::tui::search::FilteredTicket;
use crate::tui::state::Pane;
use crate::types::TicketMetadata;

use super::types::ViewAction;

/// Context struct holding all mutable state for event handlers
pub struct ViewHandlerContext<'a> {
    // Search/filter state
    pub search_query: &'a mut State<String>,
    pub pending_search: &'a mut State<bool>,

    // Navigation state
    pub selected_index: &'a mut State<usize>,
    pub scroll_offset: &'a mut State<usize>,
    pub detail_scroll_offset: &'a mut State<usize>,

    // Pane state
    pub active_pane: &'a mut State<Pane>,

    // Triage mode state
    pub is_triage_mode: bool,

    // App state
    pub should_exit: &'a mut State<bool>,
    pub needs_reload: &'a mut State<bool>,

    // Edit form state
    pub edit_result: &'a mut State<EditResult>,
    pub is_editing_existing: &'a mut State<bool>,
    pub is_creating_new: &'a mut State<bool>,
    pub editing_ticket_id: &'a mut State<String>,
    pub editing_ticket: &'a mut State<TicketMetadata>,
    pub editing_body: &'a mut State<String>,

    // Computed values (read-only)
    pub filtered_count: usize,
    pub list_height: usize,
    pub max_detail_scroll: usize,

    // Ticket data for operations
    pub filtered_tickets: &'a [FilteredTicket],

    // Async action queue sender
    pub action_tx: &'a ActionChannel<ViewAction>,
}

impl<'a> ViewHandlerContext<'a> {
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
