//! Handler context containing grouped state references
//!
//! This module organizes the TUI state into logical groups, making it easier
//! to understand which state each handler needs and simplifying testing.

use iocraft::prelude::State;

use crate::tui::action_queue::ActionChannel;
use crate::tui::edit_state::EditFormState;
use crate::tui::search::FilteredTicket;
use crate::tui::state::Pane;
use crate::types::TicketMetadata;

use super::types::ViewAction;

/// Search functionality state
pub struct SearchState<'a> {
    pub query: &'a mut State<String>,
    pub pending: &'a mut State<bool>,
}

/// List navigation state (selection and scroll in list)
pub struct ListNavigationState<'a> {
    pub selected_index: &'a mut State<usize>,
    pub scroll_offset: &'a mut State<usize>,
}

/// Detail navigation state (scroll in detail pane)
pub struct DetailNavigationState<'a> {
    pub scroll_offset: &'a mut State<usize>,
    pub max_scroll: usize,
}

/// Global app state (exit, reload, active pane, mode)
pub struct AppState<'a> {
    pub should_exit: &'a mut State<bool>,
    pub needs_reload: &'a mut State<bool>,
    pub active_pane: &'a mut State<Pane>,
    pub is_triage_mode: bool,
}

/// Data and computed values for view
pub struct ViewData<'a> {
    pub filtered_tickets: &'a [FilteredTicket],
    pub filtered_count: usize,
    pub list_height: usize,
    pub list_nav: ListNavigationState<'a>,
    pub detail_nav: DetailNavigationState<'a>,
}

/// Edit-related state
pub struct EditState<'a> {
    pub result: &'a mut State<crate::tui::edit::EditResult>,
    pub is_editing_existing: &'a mut State<bool>,
    pub is_creating_new: &'a mut State<bool>,
    pub editing_ticket_id: &'a mut State<String>,
    pub editing_ticket: &'a mut State<TicketMetadata>,
    pub editing_body: &'a mut State<String>,
}

/// Main context struct holding grouped state for event handlers
///
/// This struct organizes state into logical groups, making it easier to:
/// - Understand which state each handler needs
/// - Test handlers with only relevant state
/// - Reason about dependencies and side effects
pub struct ViewHandlerContext<'a> {
    pub search: SearchState<'a>,
    pub app: AppState<'a>,
    pub data: ViewData<'a>,
    pub edit: EditState<'a>,
    pub actions: &'a ActionChannel<ViewAction>,
}

impl<'a> ViewHandlerContext<'a> {
    /// Convenience method to create an EditFormState for edit operations
    pub fn edit_form_state(&mut self) -> EditFormState<'_> {
        EditFormState {
            result: self.edit.result,
            is_editing_existing: self.edit.is_editing_existing,
            is_creating_new: self.edit.is_creating_new,
            editing_ticket: self.edit.editing_ticket,
            editing_body: self.edit.editing_body,
        }
    }
}
