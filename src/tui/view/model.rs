//! IssueBrowser model types for testable state management
//!
//! This module separates state (ViewState) from view (ViewViewModel)
//! enabling comprehensive unit testing without the iocraft framework.

use crate::tui::components::empty_state::EmptyStateKind;
use crate::tui::components::footer::Shortcut;
use crate::tui::components::toast::Toast;
use crate::tui::components::{
    browser_shortcuts, compute_empty_state, edit_shortcuts, empty_shortcuts, search_shortcuts,
};
use crate::tui::repository::InitResult;
use crate::tui::search::{FilteredTicket, filter_tickets};
use crate::tui::state::Pane;
use crate::types::TicketMetadata;

use iocraft::prelude::{KeyCode, KeyModifiers};

/// Edit mode variants for the view
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditMode {
    /// Creating a new ticket
    Creating,
    /// Editing an existing ticket
    Editing {
        /// ID of the ticket being edited
        ticket_id: String,
    },
}

/// Raw state that changes during user interaction
#[derive(Debug, Clone, Default)]
pub struct ViewState {
    /// All tickets loaded from the repository
    pub tickets: Vec<TicketMetadata>,
    /// Current search query string
    pub search_query: String,
    /// Index of the currently selected ticket in the filtered list
    pub selected_index: usize,
    /// Scroll offset for the list view
    pub scroll_offset: usize,
    /// Currently active pane
    pub active_pane: Pane,
    /// Whether tickets are currently being loaded
    pub is_loading: bool,
    /// Result of repository initialization
    pub init_result: InitResult,
    /// Optional toast notification to display
    pub toast: Option<Toast>,
    /// Current edit mode state
    pub edit_mode: Option<EditMode>,
    /// Debounce delay in milliseconds, calculated at startup
    pub debounce_ms: u64,
    /// Timestamp of last search query change
    pub last_search_change: Option<std::time::Instant>,
    /// Whether an async search is currently in flight
    pub search_in_flight: bool,
}

/// All possible actions on the view
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewAction {
    // Navigation
    /// Move selection down one item
    MoveDown,
    /// Move selection up one item
    MoveUp,
    /// Jump to the first item
    GoToTop,
    /// Jump to the last item
    GoToBottom,
    /// Page down (half page)
    PageDown,
    /// Page up (half page)
    PageUp,

    // Pane cycling
    /// Cycle to the next pane (Search -> List -> Detail -> Search)
    CyclePaneForward,
    /// Cycle to the previous pane
    CyclePaneBackward,

    // Search
    /// Focus the search box
    FocusSearch,
    /// Update the search query text
    UpdateSearch(String),
    /// Exit search mode, keeping the query
    ExitSearch,
    /// Clear search query and exit search mode
    ClearSearchAndExit,

    // Edit
    /// Edit the currently selected ticket
    EditSelected,
    /// Create a new ticket
    CreateNew,
    /// Cancel the current edit operation
    CancelEdit,

    // App
    /// Quit the application
    Quit,
    /// Reload tickets from the repository
    Reload,
    /// Cycle status of selected ticket
    CycleStatus,
}

/// Computed view model for rendering the entire view
#[derive(Debug, Clone)]
pub struct ViewViewModel {
    /// List view model
    pub list: ListViewModel,
    /// Detail view model
    pub detail: DetailViewModel,
    /// Search view model
    pub search: SearchViewModel,
    /// Toast notification to display
    pub toast: Option<Toast>,
    /// Empty state to display (if any)
    pub empty_state: Option<EmptyStateKind>,
    /// Keyboard shortcuts to display in footer
    pub shortcuts: Vec<Shortcut>,
    /// Whether an edit form is currently open
    pub is_editing: bool,
    /// Total number of all tickets
    pub total_all_tickets: usize,
}

/// View model for the ticket list pane
#[derive(Debug, Clone)]
pub struct ListViewModel {
    /// Filtered tickets to display
    pub tickets: Vec<FilteredTicket>,
    /// Index of the selected ticket
    pub selected_index: usize,
    /// Scroll offset for virtual scrolling
    pub scroll_offset: usize,
    /// Whether the list pane is focused
    pub is_focused: bool,
    /// Number of visible items
    pub visible_count: usize,
}

/// View model for the ticket detail pane
#[derive(Debug, Clone)]
pub struct DetailViewModel {
    /// The ticket to display details for (if any)
    pub ticket: Option<TicketMetadata>,
    /// Whether the detail pane is focused
    pub is_focused: bool,
}

/// View model for the search box
#[derive(Debug, Clone)]
pub struct SearchViewModel {
    /// Current search query
    pub query: String,
    /// Whether the search box is focused
    pub is_focused: bool,
    /// Number of matching results
    pub result_count: usize,
}

// ============================================================================
// Pure Functions
// ============================================================================

/// Pure function: compute view model from state
///
/// This function takes the raw view state and produces a fully computed
/// view model that can be directly used for rendering. All the logic for
/// filtering and computing derived state lives here.
pub fn compute_view_model(state: &ViewState, list_height: usize) -> ViewViewModel {
    // Filter tickets by search query
    let filtered: Vec<FilteredTicket> = filter_tickets(&state.tickets, &state.search_query);
    let total_filtered = filtered.len();
    let total_all = state.tickets.len();

    // Compute empty state
    let empty_state = compute_empty_state(
        state.is_loading,
        state.init_result,
        total_all,
        total_filtered,
        &state.search_query,
    );

    // Determine if we should show full empty state (not no-search-results)
    let show_full_empty_state = matches!(
        empty_state,
        Some(EmptyStateKind::NoJanusDir)
            | Some(EmptyStateKind::NoTickets)
            | Some(EmptyStateKind::Loading)
    );

    // Determine if editing
    let is_editing = state.edit_mode.is_some();

    // Compute shortcuts to show
    let shortcuts = if is_editing {
        edit_shortcuts()
    } else if show_full_empty_state {
        empty_shortcuts()
    } else {
        match state.active_pane {
            Pane::Search => search_shortcuts(),
            _ => browser_shortcuts(),
        }
    };

    // Get selected ticket
    let selected_ticket = filtered
        .get(state.selected_index)
        .map(|ft| ft.ticket.clone());

    // Build list view model
    let list = ListViewModel {
        tickets: filtered.clone(),
        selected_index: state.selected_index,
        scroll_offset: state.scroll_offset,
        is_focused: state.active_pane == Pane::List && !is_editing,
        visible_count: list_height.min(total_filtered),
    };

    // Build detail view model
    let detail = DetailViewModel {
        ticket: selected_ticket.map(|t| (*t).clone()),
        is_focused: state.active_pane == Pane::Detail && !is_editing,
    };

    // Build search view model
    let search = SearchViewModel {
        query: state.search_query.clone(),
        is_focused: state.active_pane == Pane::Search && !is_editing,
        result_count: total_filtered,
    };

    ViewViewModel {
        list,
        detail,
        search,
        toast: state.toast.clone(),
        empty_state,
        shortcuts,
        is_editing,
        total_all_tickets: total_all,
    }
}

/// Pure function: apply action to state (reducer pattern)
///
/// This function takes the current state and an action, returning the new state.
/// It contains only pure state transitions - no side effects like file I/O.
///
/// Note: Some actions (like EditSelected, CycleStatus) require async I/O and
/// are handled separately by the component. This function only handles the
/// synchronous state updates.
pub fn reduce_view_state(
    mut state: ViewState,
    action: ViewAction,
    list_height: usize,
) -> ViewState {
    // Filter tickets to get the current count
    let filtered = filter_tickets(&state.tickets, &state.search_query);
    let list_count = filtered.len();

    match action {
        // Navigation
        ViewAction::MoveDown => {
            if list_count > 0 {
                let new_idx = (state.selected_index + 1).min(list_count - 1);
                state.selected_index = new_idx;
                state.scroll_offset =
                    adjust_scroll(state.scroll_offset, state.selected_index, list_height);
            }
        }
        ViewAction::MoveUp => {
            state.selected_index = state.selected_index.saturating_sub(1);
            state.scroll_offset =
                adjust_scroll(state.scroll_offset, state.selected_index, list_height);
        }
        ViewAction::GoToTop => {
            state.selected_index = 0;
            state.scroll_offset = 0;
        }
        ViewAction::GoToBottom => {
            if list_count > 0 {
                state.selected_index = list_count - 1;
                state.scroll_offset =
                    adjust_scroll(state.scroll_offset, state.selected_index, list_height);
            }
        }
        ViewAction::PageDown => {
            if list_count > 0 {
                let jump = list_height / 2;
                state.selected_index = (state.selected_index + jump).min(list_count - 1);
                state.scroll_offset =
                    adjust_scroll(state.scroll_offset, state.selected_index, list_height);
            }
        }
        ViewAction::PageUp => {
            let jump = list_height / 2;
            state.selected_index = state.selected_index.saturating_sub(jump);
            state.scroll_offset =
                adjust_scroll(state.scroll_offset, state.selected_index, list_height);
        }

        // Pane cycling
        ViewAction::CyclePaneForward => {
            state.active_pane = match state.active_pane {
                Pane::Search => Pane::List,
                Pane::List => Pane::Detail,
                Pane::Detail => Pane::Search,
            };
        }
        ViewAction::CyclePaneBackward => {
            state.active_pane = match state.active_pane {
                Pane::Search => Pane::Detail,
                Pane::List => Pane::Search,
                Pane::Detail => Pane::List,
            };
        }

        // Search
        ViewAction::FocusSearch => {
            state.active_pane = Pane::Search;
        }
        ViewAction::UpdateSearch(query) => {
            state.search_query = query;
            // Reset selection when search changes
            state.selected_index = 0;
            state.scroll_offset = 0;
        }
        ViewAction::ExitSearch => {
            state.active_pane = Pane::List;
        }
        ViewAction::ClearSearchAndExit => {
            state.search_query = String::new();
            state.active_pane = Pane::List;
            // Reset selection when search is cleared
            state.selected_index = 0;
            state.scroll_offset = 0;
        }

        // Edit (sync state only - actual I/O handled separately)
        ViewAction::CreateNew => {
            state.edit_mode = Some(EditMode::Creating);
        }
        ViewAction::CancelEdit => {
            state.edit_mode = None;
        }

        // These actions are handled by the component's async logic,
        // but we still need to match them to avoid warnings
        ViewAction::EditSelected
        | ViewAction::CycleStatus
        | ViewAction::Quit
        | ViewAction::Reload => {
            // These require async I/O or system context, handled externally
        }
    }
    state
}

/// Adjust scroll offset to keep selected item visible
///
/// Returns the new scroll offset that ensures the selected index is visible
/// within the list height.
pub fn adjust_scroll(scroll_offset: usize, selected_index: usize, list_height: usize) -> usize {
    if list_height == 0 {
        return 0;
    }

    // If selected is above the visible area, scroll up
    if selected_index < scroll_offset {
        return selected_index;
    }

    // If selected is below the visible area, scroll down
    if selected_index >= scroll_offset + list_height {
        return selected_index.saturating_sub(list_height - 1);
    }

    // Selected is within visible area, keep scroll as is
    scroll_offset
}

/// Convert a key event to a ViewAction (pure function)
///
/// This function maps keyboard events to abstract view actions, enabling
/// unit testing of the key mapping logic without any iocraft dependencies.
///
/// Returns `None` if the key doesn't map to any action.
pub fn key_to_action(
    code: KeyCode,
    modifiers: KeyModifiers,
    active_pane: Pane,
) -> Option<ViewAction> {
    // Handle search mode specially
    if active_pane == Pane::Search {
        return search_key_to_action(code, modifiers);
    }

    match code {
        // Navigation
        KeyCode::Char('j') | KeyCode::Down => Some(ViewAction::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(ViewAction::MoveUp),
        KeyCode::Char('g') => Some(ViewAction::GoToTop),
        KeyCode::Char('G') => Some(ViewAction::GoToBottom),
        KeyCode::PageDown => Some(ViewAction::PageDown),
        KeyCode::PageUp => Some(ViewAction::PageUp),

        // Pane navigation
        KeyCode::Tab => Some(ViewAction::CyclePaneForward),
        KeyCode::BackTab => Some(ViewAction::CyclePaneBackward),

        // Actions
        KeyCode::Char('q') => Some(ViewAction::Quit),
        KeyCode::Char('/') => Some(ViewAction::FocusSearch),
        KeyCode::Char('e') | KeyCode::Enter => Some(ViewAction::EditSelected),
        KeyCode::Char('n') => Some(ViewAction::CreateNew),
        KeyCode::Char('s') => Some(ViewAction::CycleStatus),
        KeyCode::Char('r') => Some(ViewAction::Reload),

        // Escape goes back to list from detail, otherwise quits
        KeyCode::Esc if active_pane == Pane::Detail => Some(ViewAction::CyclePaneBackward),
        KeyCode::Esc => Some(ViewAction::Quit),

        _ => None,
    }
}

/// Convert a key event in search mode to a ViewAction
fn search_key_to_action(code: KeyCode, modifiers: KeyModifiers) -> Option<ViewAction> {
    match (code, modifiers) {
        // Escape clears and exits
        (KeyCode::Esc, _) => Some(ViewAction::ClearSearchAndExit),
        // Enter/Tab exits keeping query
        (KeyCode::Enter, _) | (KeyCode::Tab, _) => Some(ViewAction::ExitSearch),
        // Ctrl+Q quits
        (KeyCode::Char('q'), m) if m.contains(KeyModifiers::CONTROL) => Some(ViewAction::Quit),
        // Other characters are handled by the search box component
        _ => None,
    }
}

/// Get the ticket at a specific index from the filtered list
pub fn get_ticket_at(state: &ViewState, index: usize) -> Option<TicketMetadata> {
    let filtered = filter_tickets(&state.tickets, &state.search_query);
    filtered.get(index).map(|ft| ft.ticket.as_ref().clone())
}

/// Get the currently selected ticket
pub fn get_selected_ticket(state: &ViewState) -> Option<TicketMetadata> {
    get_ticket_at(state, state.selected_index)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TicketPriority, TicketStatus, TicketType};

    fn make_ticket(id: &str, title: &str, status: TicketStatus) -> TicketMetadata {
        TicketMetadata {
            id: Some(id.to_string()),
            title: Some(title.to_string()),
            status: Some(status),
            priority: Some(TicketPriority::P2),
            ticket_type: Some(TicketType::Task),
            ..Default::default()
        }
    }

    fn default_state() -> ViewState {
        ViewState {
            tickets: vec![],
            search_query: String::new(),
            selected_index: 0,
            scroll_offset: 0,
            active_pane: Pane::List,
            is_loading: false,
            init_result: InitResult::Ok,
            toast: None,
            edit_mode: None,
            debounce_ms: 10,
            last_search_change: None,
            search_in_flight: false,
        }
    }

    fn state_with_tickets(count: usize) -> ViewState {
        let tickets: Vec<TicketMetadata> = (0..count)
            .map(|i| make_ticket(&format!("j-{i}"), &format!("Task {i}"), TicketStatus::New))
            .collect();
        ViewState {
            tickets,
            ..default_state()
        }
    }

    // ========================================================================
    // Reducer Tests - Navigation
    // ========================================================================

    #[test]
    fn test_reduce_move_down() {
        let state = state_with_tickets(5);
        let new_state = reduce_view_state(state, ViewAction::MoveDown, 10);
        assert_eq!(new_state.selected_index, 1);
    }

    #[test]
    fn test_reduce_move_down_at_bottom() {
        let mut state = state_with_tickets(5);
        state.selected_index = 4; // Last item
        let new_state = reduce_view_state(state, ViewAction::MoveDown, 10);
        assert_eq!(new_state.selected_index, 4); // Should stay at bottom
    }

    #[test]
    fn test_reduce_move_up() {
        let mut state = state_with_tickets(5);
        state.selected_index = 2;
        let new_state = reduce_view_state(state, ViewAction::MoveUp, 10);
        assert_eq!(new_state.selected_index, 1);
    }

    #[test]
    fn test_reduce_move_up_at_top() {
        let state = state_with_tickets(5);
        let new_state = reduce_view_state(state, ViewAction::MoveUp, 10);
        assert_eq!(new_state.selected_index, 0); // Should stay at top
    }

    #[test]
    fn test_reduce_go_to_top() {
        let mut state = state_with_tickets(5);
        state.selected_index = 3;
        state.scroll_offset = 2;
        let new_state = reduce_view_state(state, ViewAction::GoToTop, 10);
        assert_eq!(new_state.selected_index, 0);
        assert_eq!(new_state.scroll_offset, 0);
    }

    #[test]
    fn test_reduce_go_to_bottom() {
        let state = state_with_tickets(5);
        let new_state = reduce_view_state(state, ViewAction::GoToBottom, 10);
        assert_eq!(new_state.selected_index, 4);
    }

    #[test]
    fn test_reduce_page_down() {
        let state = state_with_tickets(20);
        let new_state = reduce_view_state(state, ViewAction::PageDown, 10);
        assert_eq!(new_state.selected_index, 5); // Half page (10/2)
    }

    #[test]
    fn test_reduce_page_up() {
        let mut state = state_with_tickets(20);
        state.selected_index = 10;
        let new_state = reduce_view_state(state, ViewAction::PageUp, 10);
        assert_eq!(new_state.selected_index, 5); // Half page up
    }

    // ========================================================================
    // Reducer Tests - Pane Cycling
    // ========================================================================

    #[test]
    fn test_reduce_cycle_pane_forward() {
        let mut state = default_state();
        state.active_pane = Pane::Search;
        let new_state = reduce_view_state(state, ViewAction::CyclePaneForward, 10);
        assert_eq!(new_state.active_pane, Pane::List);

        let new_state = reduce_view_state(new_state, ViewAction::CyclePaneForward, 10);
        assert_eq!(new_state.active_pane, Pane::Detail);

        let new_state = reduce_view_state(new_state, ViewAction::CyclePaneForward, 10);
        assert_eq!(new_state.active_pane, Pane::Search);
    }

    #[test]
    fn test_reduce_cycle_pane_backward() {
        let mut state = default_state();
        state.active_pane = Pane::Search;
        let new_state = reduce_view_state(state, ViewAction::CyclePaneBackward, 10);
        assert_eq!(new_state.active_pane, Pane::Detail);

        let new_state = reduce_view_state(new_state, ViewAction::CyclePaneBackward, 10);
        assert_eq!(new_state.active_pane, Pane::List);

        let new_state = reduce_view_state(new_state, ViewAction::CyclePaneBackward, 10);
        assert_eq!(new_state.active_pane, Pane::Search);
    }

    // ========================================================================
    // Reducer Tests - Search
    // ========================================================================

    #[test]
    fn test_reduce_focus_search() {
        let state = default_state();
        let new_state = reduce_view_state(state, ViewAction::FocusSearch, 10);
        assert_eq!(new_state.active_pane, Pane::Search);
    }

    #[test]
    fn test_reduce_update_search() {
        let state = default_state();
        let new_state = reduce_view_state(state, ViewAction::UpdateSearch("bug".to_string()), 10);
        assert_eq!(new_state.search_query, "bug");
    }

    #[test]
    fn test_reduce_search_resets_selection() {
        let mut state = state_with_tickets(10);
        state.selected_index = 5;
        state.scroll_offset = 3;
        let new_state = reduce_view_state(state, ViewAction::UpdateSearch("test".to_string()), 10);
        assert_eq!(new_state.selected_index, 0);
        assert_eq!(new_state.scroll_offset, 0);
    }

    #[test]
    fn test_reduce_exit_search() {
        let mut state = default_state();
        state.active_pane = Pane::Search;
        state.search_query = "test".to_string();
        let new_state = reduce_view_state(state, ViewAction::ExitSearch, 10);
        assert_eq!(new_state.active_pane, Pane::List);
        assert_eq!(new_state.search_query, "test"); // Query preserved
    }

    #[test]
    fn test_reduce_clear_search_and_exit() {
        let mut state = default_state();
        state.active_pane = Pane::Search;
        state.search_query = "test".to_string();
        let new_state = reduce_view_state(state, ViewAction::ClearSearchAndExit, 10);
        assert_eq!(new_state.active_pane, Pane::List);
        assert_eq!(new_state.search_query, ""); // Query cleared
    }

    // ========================================================================
    // Reducer Tests - Edit Mode
    // ========================================================================

    #[test]
    fn test_reduce_create_new() {
        let state = default_state();
        let new_state = reduce_view_state(state, ViewAction::CreateNew, 10);
        assert_eq!(new_state.edit_mode, Some(EditMode::Creating));
    }

    #[test]
    fn test_reduce_cancel_edit() {
        let mut state = default_state();
        state.edit_mode = Some(EditMode::Creating);
        let new_state = reduce_view_state(state, ViewAction::CancelEdit, 10);
        assert_eq!(new_state.edit_mode, None);
    }

    // ========================================================================
    // Key Mapping Tests
    // ========================================================================

    #[test]
    fn test_key_to_action_navigation() {
        assert_eq!(
            key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, Pane::List),
            Some(ViewAction::MoveDown)
        );
        assert_eq!(
            key_to_action(KeyCode::Down, KeyModifiers::NONE, Pane::List),
            Some(ViewAction::MoveDown)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('k'), KeyModifiers::NONE, Pane::List),
            Some(ViewAction::MoveUp)
        );
        assert_eq!(
            key_to_action(KeyCode::Up, KeyModifiers::NONE, Pane::List),
            Some(ViewAction::MoveUp)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('g'), KeyModifiers::NONE, Pane::List),
            Some(ViewAction::GoToTop)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('G'), KeyModifiers::NONE, Pane::List),
            Some(ViewAction::GoToBottom)
        );
        assert_eq!(
            key_to_action(KeyCode::PageDown, KeyModifiers::NONE, Pane::List),
            Some(ViewAction::PageDown)
        );
        assert_eq!(
            key_to_action(KeyCode::PageUp, KeyModifiers::NONE, Pane::List),
            Some(ViewAction::PageUp)
        );
    }

    #[test]
    fn test_key_to_action_pane_navigation() {
        assert_eq!(
            key_to_action(KeyCode::Tab, KeyModifiers::NONE, Pane::List),
            Some(ViewAction::CyclePaneForward)
        );
        assert_eq!(
            key_to_action(KeyCode::BackTab, KeyModifiers::NONE, Pane::List),
            Some(ViewAction::CyclePaneBackward)
        );
    }

    #[test]
    fn test_key_to_action_app_commands() {
        assert_eq!(
            key_to_action(KeyCode::Char('q'), KeyModifiers::NONE, Pane::List),
            Some(ViewAction::Quit)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('/'), KeyModifiers::NONE, Pane::List),
            Some(ViewAction::FocusSearch)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('e'), KeyModifiers::NONE, Pane::List),
            Some(ViewAction::EditSelected)
        );
        assert_eq!(
            key_to_action(KeyCode::Enter, KeyModifiers::NONE, Pane::List),
            Some(ViewAction::EditSelected)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('n'), KeyModifiers::NONE, Pane::List),
            Some(ViewAction::CreateNew)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('s'), KeyModifiers::NONE, Pane::List),
            Some(ViewAction::CycleStatus)
        );
        assert_eq!(
            key_to_action(KeyCode::Char('r'), KeyModifiers::NONE, Pane::List),
            Some(ViewAction::Reload)
        );
    }

    #[test]
    fn test_key_to_action_detail_escape() {
        // Escape in detail pane should go back
        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, Pane::Detail),
            Some(ViewAction::CyclePaneBackward)
        );
    }

    #[test]
    fn test_key_to_action_search_mode() {
        // Escape clears and exits
        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, Pane::Search),
            Some(ViewAction::ClearSearchAndExit)
        );
        // Enter exits keeping query
        assert_eq!(
            key_to_action(KeyCode::Enter, KeyModifiers::NONE, Pane::Search),
            Some(ViewAction::ExitSearch)
        );
        // Tab exits keeping query
        assert_eq!(
            key_to_action(KeyCode::Tab, KeyModifiers::NONE, Pane::Search),
            Some(ViewAction::ExitSearch)
        );
        // Ctrl+Q quits
        assert_eq!(
            key_to_action(KeyCode::Char('q'), KeyModifiers::CONTROL, Pane::Search),
            Some(ViewAction::Quit)
        );
    }

    #[test]
    fn test_key_to_action_search_mode_regular_keys() {
        // Regular keys in search mode return None (handled by search box)
        assert_eq!(
            key_to_action(KeyCode::Char('a'), KeyModifiers::NONE, Pane::Search),
            None
        );
        assert_eq!(
            key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, Pane::Search),
            None
        );
    }

    #[test]
    fn test_key_to_action_unknown_keys() {
        assert_eq!(
            key_to_action(KeyCode::Char('x'), KeyModifiers::NONE, Pane::List),
            None
        );
        assert_eq!(
            key_to_action(KeyCode::F(1), KeyModifiers::NONE, Pane::List),
            None
        );
    }

    // ========================================================================
    // View Model Tests
    // ========================================================================

    #[test]
    fn test_compute_view_model_empty() {
        let state = default_state();
        let vm = compute_view_model(&state, 10);

        assert_eq!(vm.total_all_tickets, 0);
        assert_eq!(vm.list.tickets.len(), 0);
        assert!(vm.detail.ticket.is_none());
        assert_eq!(vm.empty_state, Some(EmptyStateKind::NoTickets));
        assert!(!vm.is_editing);
    }

    #[test]
    fn test_compute_view_model_with_tickets() {
        let state = state_with_tickets(5);
        let vm = compute_view_model(&state, 10);

        assert_eq!(vm.total_all_tickets, 5);
        assert_eq!(vm.list.tickets.len(), 5);
        assert!(vm.detail.ticket.is_some());
        assert!(vm.empty_state.is_none());
    }

    #[test]
    fn test_compute_view_model_with_search() {
        let tickets = vec![
            make_ticket("j-1", "Fix login bug", TicketStatus::New),
            make_ticket("j-2", "Add feature", TicketStatus::New),
            make_ticket("j-3", "Another bug fix", TicketStatus::InProgress),
        ];
        let state = ViewState {
            tickets,
            search_query: "bug".to_string(),
            ..default_state()
        };
        let vm = compute_view_model(&state, 10);

        assert_eq!(vm.total_all_tickets, 3);
        assert_eq!(vm.list.tickets.len(), 2); // Only "bug" tickets
        assert_eq!(vm.search.query, "bug");
        assert_eq!(vm.search.result_count, 2);
    }

    #[test]
    fn test_compute_view_model_loading() {
        let state = ViewState {
            is_loading: true,
            ..default_state()
        };
        let vm = compute_view_model(&state, 10);

        assert_eq!(vm.empty_state, Some(EmptyStateKind::Loading));
    }

    #[test]
    fn test_compute_view_model_no_janus_dir() {
        let state = ViewState {
            init_result: InitResult::NoJanusDir,
            ..default_state()
        };
        let vm = compute_view_model(&state, 10);

        assert_eq!(vm.empty_state, Some(EmptyStateKind::NoJanusDir));
    }

    #[test]
    fn test_compute_view_model_editing_mode() {
        let mut state = state_with_tickets(3);
        state.edit_mode = Some(EditMode::Creating);
        let vm = compute_view_model(&state, 10);

        assert!(vm.is_editing);
        // List and detail should not be focused when editing
        assert!(!vm.list.is_focused);
        assert!(!vm.detail.is_focused);
    }

    #[test]
    fn test_compute_view_model_pane_focus() {
        let mut state = state_with_tickets(3);

        // Search pane focused
        state.active_pane = Pane::Search;
        let vm = compute_view_model(&state, 10);
        assert!(vm.search.is_focused);
        assert!(!vm.list.is_focused);
        assert!(!vm.detail.is_focused);

        // List pane focused
        state.active_pane = Pane::List;
        let vm = compute_view_model(&state, 10);
        assert!(!vm.search.is_focused);
        assert!(vm.list.is_focused);
        assert!(!vm.detail.is_focused);

        // Detail pane focused
        state.active_pane = Pane::Detail;
        let vm = compute_view_model(&state, 10);
        assert!(!vm.search.is_focused);
        assert!(!vm.list.is_focused);
        assert!(vm.detail.is_focused);
    }

    // ========================================================================
    // Helper Function Tests
    // ========================================================================

    #[test]
    fn test_adjust_scroll_within_bounds() {
        // Selected is within visible area, scroll stays the same
        assert_eq!(adjust_scroll(0, 3, 10), 0);
        assert_eq!(adjust_scroll(5, 8, 10), 5);
    }

    #[test]
    fn test_adjust_scroll_below_visible() {
        // Selected is below visible area, scroll down
        assert_eq!(adjust_scroll(0, 15, 10), 6);
        assert_eq!(adjust_scroll(5, 20, 10), 11);
    }

    #[test]
    fn test_adjust_scroll_above_visible() {
        // Selected is above visible area, scroll up
        assert_eq!(adjust_scroll(10, 5, 10), 5);
        assert_eq!(adjust_scroll(20, 10, 10), 10);
    }

    #[test]
    fn test_adjust_scroll_zero_height() {
        // Zero height should return 0
        assert_eq!(adjust_scroll(5, 10, 0), 0);
    }

    #[test]
    fn test_get_ticket_at() {
        let state = state_with_tickets(5);

        let ticket = get_ticket_at(&state, 2);
        assert!(ticket.is_some());
        assert_eq!(ticket.unwrap().id, Some("j-2".to_string()));

        // Out of bounds
        assert!(get_ticket_at(&state, 10).is_none());
    }

    #[test]
    fn test_get_selected_ticket() {
        let mut state = state_with_tickets(5);
        state.selected_index = 3;

        let ticket = get_selected_ticket(&state);
        assert!(ticket.is_some());
        assert_eq!(ticket.unwrap().id, Some("j-3".to_string()));
    }

    // ========================================================================
    // Edit Mode Equality Tests
    // ========================================================================

    #[test]
    fn test_edit_mode_equality() {
        assert_eq!(EditMode::Creating, EditMode::Creating);
        assert_eq!(
            EditMode::Editing {
                ticket_id: "j-123".to_string()
            },
            EditMode::Editing {
                ticket_id: "j-123".to_string()
            }
        );
        assert_ne!(
            EditMode::Creating,
            EditMode::Editing {
                ticket_id: "j-123".to_string()
            }
        );
    }

    // ========================================================================
    // Scroll Adjustment Tests
    // ========================================================================

    #[test]
    fn test_view_scroll_down_adjusts_offset() {
        let mut state = ViewState {
            tickets: (0..30)
                .map(|i| make_ticket(&format!("j-{i}"), &format!("Task {i}"), TicketStatus::New))
                .collect(),
            selected_index: 9, // At the boundary of visible area
            scroll_offset: 0,
            ..default_state()
        };

        // Move down with list_height=10 should scroll when going past visible
        state = reduce_view_state(state, ViewAction::MoveDown, 10);
        assert_eq!(state.selected_index, 10);
        assert_eq!(state.scroll_offset, 1, "Should scroll down");
    }

    #[test]
    fn test_view_scroll_up_adjusts_offset() {
        let mut state = ViewState {
            tickets: (0..30)
                .map(|i| make_ticket(&format!("j-{i}"), &format!("Task {i}"), TicketStatus::New))
                .collect(),
            selected_index: 10,
            scroll_offset: 10, // Scrolled down
            ..default_state()
        };

        // Move up should scroll up
        state = reduce_view_state(state, ViewAction::MoveUp, 10);
        assert_eq!(state.selected_index, 9);
        assert_eq!(state.scroll_offset, 9, "Should scroll up");
    }

    #[test]
    fn test_view_go_to_bottom_large_list() {
        let mut state = ViewState {
            tickets: (0..100)
                .map(|i| make_ticket(&format!("j-{i}"), &format!("Task {i}"), TicketStatus::New))
                .collect(),
            selected_index: 0,
            scroll_offset: 0,
            ..default_state()
        };

        state = reduce_view_state(state, ViewAction::GoToBottom, 20);
        assert_eq!(state.selected_index, 99);
        // scroll_offset should position so last item is visible
        assert!(state.scroll_offset >= 80, "Should scroll to show bottom");
    }

    #[test]
    fn test_view_page_down_large_list() {
        let mut state = ViewState {
            tickets: (0..100)
                .map(|i| make_ticket(&format!("j-{i}"), &format!("Task {i}"), TicketStatus::New))
                .collect(),
            selected_index: 0,
            scroll_offset: 0,
            ..default_state()
        };

        // Page down with height 20 should move 10 (half page)
        state = reduce_view_state(state, ViewAction::PageDown, 20);
        assert_eq!(state.selected_index, 10);
    }

    #[test]
    fn test_view_model_scroll_info() {
        let state = ViewState {
            tickets: (0..50)
                .map(|i| make_ticket(&format!("j-{i}"), &format!("Task {i}"), TicketStatus::New))
                .collect(),
            selected_index: 25,
            scroll_offset: 20,
            ..default_state()
        };

        let vm = compute_view_model(&state, 10);
        assert_eq!(vm.list.scroll_offset, 20);
        assert_eq!(vm.list.selected_index, 25);
    }
}
