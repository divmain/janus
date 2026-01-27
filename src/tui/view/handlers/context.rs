//! Handler context containing grouped state references
//!
//! This module organizes the TUI state into logical groups, making it easier
//! to understand which state each handler needs and simplifying testing.

use iocraft::prelude::{Handler, State};

use crate::tui::edit::EditResult;
use crate::tui::edit_state::{EditFormState, EditMode};
use crate::tui::search::FilteredTicket;
use crate::tui::search_orchestrator::SearchState as SearchOrchestrator;
use crate::tui::state::Pane;

/// Search functionality state
pub struct SearchState<'a> {
    pub query: &'a mut State<String>,
    pub orchestrator: &'a mut SearchOrchestrator,
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
    pub mode: &'a mut State<EditMode>,
    pub result: &'a mut State<EditResult>,
}

/// Async handlers for ticket operations
pub struct AsyncHandlers<'a> {
    pub cycle_status: &'a Handler<String>,
    pub mark_triaged: &'a Handler<(String, bool)>,
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
    pub handlers: AsyncHandlers<'a>,
}

impl<'a> ViewHandlerContext<'a> {
    /// Convenience method to create an EditFormState for edit operations
    pub fn edit_form_state(&mut self) -> EditFormState<'_> {
        EditFormState {
            mode: self.edit.mode,
            result: self.edit.result,
        }
    }
}
