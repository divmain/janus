//! KanbanBoard model types for testable state management
//!
//! This module separates state (BoardState) from view (BoardViewModel)
//! enabling comprehensive unit testing without the iocraft framework.

use crate::tui::components::empty_state::EmptyStateKind;
use crate::tui::components::footer::Shortcut;
use crate::tui::components::toast::Toast;
use crate::tui::components::{
    board_shortcuts, compute_empty_state, edit_shortcuts, empty_shortcuts,
};
use crate::tui::repository::InitResult;
use crate::tui::search::{FilteredTicket, filter_tickets};
use crate::types::{TicketMetadata, TicketStatus};

// Column configuration constants
/// The 5 kanban columns in order
pub const COLUMNS: [TicketStatus; 5] = [
    TicketStatus::New,
    TicketStatus::Next,
    TicketStatus::InProgress,
    TicketStatus::Complete,
    TicketStatus::Cancelled,
];

/// Column display names
pub const COLUMN_NAMES: [&str; 5] = ["NEW", "NEXT", "IN PROGRESS", "COMPLETE", "CANCELLED"];

/// Column toggle keys for header display
pub const COLUMN_KEYS: [char; 5] = ['N', 'X', 'I', 'C', '_'];

/// Raw state that changes during user interaction
#[derive(Debug, Clone, Default)]
pub struct BoardState {
    /// All tickets loaded from the repository
    pub tickets: Vec<TicketMetadata>,
    /// Current search query string
    pub search_query: String,
    /// Whether the search box is focused
    pub search_focused: bool,
    /// Index of the currently selected column (0-4)
    pub current_column: usize,
    /// Index of the currently selected row within the column
    pub current_row: usize,
    /// Visibility state for each column
    pub visible_columns: [bool; 5],
    /// Scroll offset for each column (index of first visible card)
    pub column_scroll_offsets: [usize; 5],
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

/// Edit mode variants
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

/// All possible actions on the board
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardAction {
    // Navigation
    /// Move selection to the left column
    MoveLeft,
    /// Move selection to the right column
    MoveRight,
    /// Move selection up within the column
    MoveUp,
    /// Move selection down within the column
    MoveDown,
    /// Jump to top of column
    GoToTop,
    /// Jump to bottom of column
    GoToBottom,
    /// Page down within column
    PageDown,
    /// Page up within column
    PageUp,

    // Column visibility
    /// Toggle visibility of a column by index (0-4)
    ToggleColumn(usize),

    // Ticket status changes
    /// Move selected ticket to the next status (right)
    MoveTicketStatusRight,
    /// Move selected ticket to the previous status (left)
    MoveTicketStatusLeft,

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

    // Actions
    /// Copy the selected ticket's ID to clipboard
    CopyTicketId,

    // App
    /// Quit the application
    Quit,
    /// Reload tickets from the repository
    Reload,
}

/// Computed view model for rendering
#[derive(Debug, Clone)]
pub struct BoardViewModel {
    /// Column view models for each visible column
    pub columns: Vec<ColumnViewModel>,
    /// Search box state
    pub search: SearchViewModel,
    /// Currently selected ticket (if any)
    pub selected_ticket: Option<TicketMetadata>,
    /// Toast notification to display
    pub toast: Option<Toast>,
    /// Empty state to display (if any)
    pub empty_state: Option<EmptyStateKind>,
    /// Keyboard shortcuts to display in footer
    pub shortcuts: Vec<Shortcut>,
    /// Whether an edit form is currently open
    pub is_editing: bool,
    /// Total number of filtered tickets
    pub total_filtered_tickets: usize,
    /// Total number of all tickets
    pub total_all_tickets: usize,
    /// Column toggle indicator string (e.g., "[N][X][I][C][_]")
    pub column_toggles: String,
}

/// View model for a single column
#[derive(Debug, Clone)]
pub struct ColumnViewModel {
    /// Status this column represents
    pub status: TicketStatus,
    /// Display name of the column
    pub name: &'static str,
    /// Key to toggle this column's visibility
    pub toggle_key: char,
    /// Whether this column is visible
    pub is_visible: bool,
    /// Whether this column is currently selected
    pub is_active: bool,
    /// Number of tickets in this column
    pub ticket_count: usize,
    /// Cards to display in this column
    pub cards: Vec<CardViewModel>,
    /// Scroll offset for this column (first visible row index)
    pub scroll_offset: usize,
    /// Number of rows actually visible (after scrolling/truncation)
    pub visible_row_count: usize,
    /// Number of tickets above the visible area
    pub hidden_above: usize,
    /// Number of tickets below the visible area
    pub hidden_below: usize,
}

/// View model for a single ticket card
#[derive(Debug, Clone)]
pub struct CardViewModel {
    /// The filtered ticket with match info
    pub ticket: FilteredTicket,
    /// Whether this card is currently selected
    pub is_selected: bool,
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
/// This function takes the raw board state and produces a fully computed
/// view model that can be directly used for rendering. All the logic for
/// filtering, grouping, and computing derived state lives here.
///
/// The `column_height` parameter specifies the number of visible cards per column,
/// used to compute scroll-related view model fields.
pub fn compute_board_view_model(state: &BoardState, column_height: usize) -> BoardViewModel {
    // Filter tickets by search query
    let filtered: Vec<FilteredTicket> = filter_tickets(&state.tickets, &state.search_query);
    let total_filtered = filtered.len();
    let total_all = state.tickets.len();

    // Group by status
    let tickets_by_status: Vec<Vec<FilteredTicket>> = COLUMNS
        .iter()
        .map(|status| get_column_tickets(&filtered, *status))
        .collect();

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
        board_shortcuts()
    };

    // Build column toggle indicator string
    let column_toggles: String = state
        .visible_columns
        .iter()
        .enumerate()
        .map(|(i, &visible)| {
            if visible {
                format!("[{}]", COLUMN_KEYS[i])
            } else {
                "[ ]".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("");

    // Build column view models
    let columns: Vec<ColumnViewModel> = COLUMNS
        .iter()
        .enumerate()
        .filter(|(i, _)| state.visible_columns[*i])
        .map(|(col_idx, &status)| {
            let column_tickets = &tickets_by_status[col_idx];
            let is_active = state.current_column == col_idx && !state.search_focused;

            let scroll_offset = state.column_scroll_offsets[col_idx];
            let total_count = column_tickets.len();

            // Calculate what's visible
            let start = scroll_offset.min(total_count);
            let end = (scroll_offset + column_height).min(total_count);

            let cards: Vec<CardViewModel> = column_tickets
                .iter()
                .enumerate()
                .skip(start)
                .take(end - start)
                .map(|(row_idx, ft)| CardViewModel {
                    ticket: ft.clone(),
                    is_selected: is_active && row_idx == state.current_row,
                })
                .collect();

            let visible_row_count = cards.len();

            ColumnViewModel {
                status,
                name: COLUMN_NAMES[col_idx],
                toggle_key: COLUMN_KEYS[col_idx],
                is_visible: true,
                is_active,
                ticket_count: total_count,
                cards,
                scroll_offset,
                visible_row_count,
                hidden_above: start,
                hidden_below: total_count.saturating_sub(end),
            }
        })
        .collect();

    // Get selected ticket
    let selected_ticket = get_selected_ticket(state, &tickets_by_status);

    // Build search view model
    let search = SearchViewModel {
        query: state.search_query.clone(),
        is_focused: state.search_focused && !is_editing,
        result_count: total_filtered,
    };

    BoardViewModel {
        columns,
        search,
        selected_ticket,
        toast: state.toast.clone(),
        empty_state,
        shortcuts,
        is_editing,
        total_filtered_tickets: total_filtered,
        total_all_tickets: total_all,
        column_toggles,
    }
}

/// Adjust scroll offset to keep selected row vertically centered.
///
/// Centers the selected row in the visible area when possible.
/// Clamps to valid scroll bounds (0 to max_scroll) when near the top or bottom.
fn adjust_column_scroll(
    _scroll_offset: usize,
    selected_row: usize,
    column_height: usize,
    total_items: usize,
) -> usize {
    if column_height == 0 || total_items == 0 {
        return 0;
    }

    // Calculate ideal scroll offset to center the selected row
    let half_height = column_height / 2;
    let ideal_offset = selected_row.saturating_sub(half_height);

    // Calculate maximum valid scroll offset
    let max_offset = total_items.saturating_sub(column_height);

    // Clamp to valid range [0, max_offset]
    ideal_offset.min(max_offset)
}

/// Pure function: apply action to state (reducer pattern)
///
/// This function takes the current state and an action, returning the new state.
/// It contains only pure state transitions - no side effects like file I/O.
///
/// Note: Some actions (like EditSelected, MoveTicketStatusRight) require async
/// I/O and are handled separately by the component. This function only handles
/// the synchronous state updates.
///
/// The `column_height` parameter specifies the number of visible cards per column,
/// used to adjust scroll offsets when navigating.
pub fn reduce_board_state(
    mut state: BoardState,
    action: BoardAction,
    column_height: usize,
) -> BoardState {
    match action {
        // Navigation
        BoardAction::MoveLeft => {
            let new_col = find_prev_visible_column(&state.visible_columns, state.current_column);
            state.current_column = new_col;
            // Adjust row for new column
            let max_row = get_column_ticket_count(&state.tickets, &state.search_query, new_col)
                .saturating_sub(1);
            if state.current_row > max_row {
                state.current_row = max_row;
            }
            // Adjust scroll for new column
            let total_items = get_column_ticket_count(&state.tickets, &state.search_query, new_col);
            state.column_scroll_offsets[new_col] = adjust_column_scroll(
                state.column_scroll_offsets[new_col],
                state.current_row,
                column_height,
                total_items,
            );
        }
        BoardAction::MoveRight => {
            let new_col = find_next_visible_column(&state.visible_columns, state.current_column);
            state.current_column = new_col;
            // Adjust row for new column
            let max_row = get_column_ticket_count(&state.tickets, &state.search_query, new_col)
                .saturating_sub(1);
            if state.current_row > max_row {
                state.current_row = max_row;
            }
            // Adjust scroll for new column
            let total_items = get_column_ticket_count(&state.tickets, &state.search_query, new_col);
            state.column_scroll_offsets[new_col] = adjust_column_scroll(
                state.column_scroll_offsets[new_col],
                state.current_row,
                column_height,
                total_items,
            );
        }
        BoardAction::MoveUp => {
            state.current_row = state.current_row.saturating_sub(1);
            let col = state.current_column;
            let total_items = get_column_ticket_count(&state.tickets, &state.search_query, col);
            state.column_scroll_offsets[col] = adjust_column_scroll(
                state.column_scroll_offsets[col],
                state.current_row,
                column_height,
                total_items,
            );
        }
        BoardAction::MoveDown => {
            let col = state.current_column;
            let total_items = get_column_ticket_count(&state.tickets, &state.search_query, col);
            let max_row = total_items.saturating_sub(1);
            state.current_row = (state.current_row + 1).min(max_row);
            state.column_scroll_offsets[col] = adjust_column_scroll(
                state.column_scroll_offsets[col],
                state.current_row,
                column_height,
                total_items,
            );
        }

        // Column visibility
        BoardAction::ToggleColumn(idx) => {
            if idx < 5 {
                state.visible_columns[idx] = !state.visible_columns[idx];
                // Adjust column if current became hidden
                if !state.visible_columns[state.current_column]
                    && let Some(first_visible) = state.visible_columns.iter().position(|&v| v)
                {
                    state.current_column = first_visible;
                }
            }
        }

        // Search
        BoardAction::FocusSearch => {
            state.search_focused = true;
        }
        BoardAction::UpdateSearch(query) => {
            state.search_query = query;
        }
        BoardAction::ExitSearch => {
            state.search_focused = false;
        }
        BoardAction::ClearSearchAndExit => {
            state.search_query = String::new();
            state.search_focused = false;
        }

        // Edit (sync state only - actual I/O handled separately)
        BoardAction::CreateNew => {
            state.edit_mode = Some(EditMode::Creating);
        }
        BoardAction::CancelEdit => {
            state.edit_mode = None;
        }

        // These actions are handled by the component's async logic,
        // but we still need to match them to avoid warnings
        BoardAction::EditSelected
        | BoardAction::MoveTicketStatusRight
        | BoardAction::MoveTicketStatusLeft
        | BoardAction::CopyTicketId
        | BoardAction::Quit
        | BoardAction::Reload => {
            // These require async I/O or system context, handled externally
        }

        // Scroll navigation
        BoardAction::GoToTop => {
            let col = state.current_column;
            state.current_row = 0;
            state.column_scroll_offsets[col] = 0;
        }
        BoardAction::GoToBottom => {
            let col = state.current_column;
            let total_items = get_column_ticket_count(&state.tickets, &state.search_query, col);
            let max_row = total_items.saturating_sub(1);
            state.current_row = max_row;
            state.column_scroll_offsets[col] = adjust_column_scroll(
                state.column_scroll_offsets[col],
                state.current_row,
                column_height,
                total_items,
            );
        }
        BoardAction::PageDown => {
            let col = state.current_column;
            let total_items = get_column_ticket_count(&state.tickets, &state.search_query, col);
            let max_row = total_items.saturating_sub(1);
            let jump = column_height / 2;
            state.current_row = (state.current_row + jump).min(max_row);
            state.column_scroll_offsets[col] = adjust_column_scroll(
                state.column_scroll_offsets[col],
                state.current_row,
                column_height,
                total_items,
            );
        }
        BoardAction::PageUp => {
            let col = state.current_column;
            let total_items = get_column_ticket_count(&state.tickets, &state.search_query, col);
            let jump = column_height / 2;
            state.current_row = state.current_row.saturating_sub(jump);
            state.column_scroll_offsets[col] = adjust_column_scroll(
                state.column_scroll_offsets[col],
                state.current_row,
                column_height,
                total_items,
            );
        }
    }
    state
}

/// Find the next visible column, wrapping around if necessary
///
/// Returns the same column if no other columns are visible.
pub fn find_next_visible_column(visible: &[bool; 5], current: usize) -> usize {
    let visible_idx: Vec<usize> = visible
        .iter()
        .enumerate()
        .filter_map(|(i, &v)| if v { Some(i) } else { None })
        .collect();

    if visible_idx.is_empty() {
        return current;
    }

    let curr_pos = visible_idx.iter().position(|&i| i == current).unwrap_or(0);

    if curr_pos < visible_idx.len() - 1 {
        visible_idx[curr_pos + 1]
    } else {
        // Don't wrap - stay at current position
        current
    }
}

/// Find the previous visible column, wrapping around if necessary
///
/// Returns the same column if no other columns are visible.
pub fn find_prev_visible_column(visible: &[bool; 5], current: usize) -> usize {
    let visible_idx: Vec<usize> = visible
        .iter()
        .enumerate()
        .filter_map(|(i, &v)| if v { Some(i) } else { None })
        .collect();

    if visible_idx.is_empty() {
        return current;
    }

    let curr_pos = visible_idx.iter().position(|&i| i == current).unwrap_or(0);

    if curr_pos > 0 {
        visible_idx[curr_pos - 1]
    } else {
        // Don't wrap - stay at current position
        current
    }
}

/// Get tickets for a specific column from the filtered list
fn get_column_tickets(filtered: &[FilteredTicket], status: TicketStatus) -> Vec<FilteredTicket> {
    filtered
        .iter()
        .filter(|ft| ft.ticket.status.unwrap_or_default() == status)
        .cloned()
        .collect()
}

/// Get the count of tickets in a specific column
///
/// Can be called with either:
/// - A BoardState reference (from model code)
/// - Individual tickets and search query (from handlers)
pub fn get_column_ticket_count(
    tickets: &[TicketMetadata],
    search_query: &str,
    column: usize,
) -> usize {
    if column >= COLUMNS.len() {
        return 0;
    }

    let filtered = filter_tickets(tickets, search_query);
    let status = COLUMNS[column];

    filtered
        .iter()
        .filter(|ft| ft.ticket.status.unwrap_or_default() == status)
        .count()
}

/// Get the currently selected ticket based on state
fn get_selected_ticket(
    state: &BoardState,
    tickets_by_status: &[Vec<FilteredTicket>],
) -> Option<TicketMetadata> {
    if state.current_column >= tickets_by_status.len() {
        return None;
    }

    let column_tickets = &tickets_by_status[state.current_column];
    column_tickets
        .get(state.current_row)
        .map(|ft| ft.ticket.as_ref().clone())
}

/// Get the ticket at a specific column and row position
pub fn get_ticket_at(state: &BoardState, column: usize, row: usize) -> Option<TicketMetadata> {
    if column >= COLUMNS.len() {
        return None;
    }

    let filtered = filter_tickets(&state.tickets, &state.search_query);
    let status = COLUMNS[column];

    let column_tickets: Vec<_> = filtered
        .iter()
        .filter(|ft| ft.ticket.status.unwrap_or_default() == status)
        .collect();

    column_tickets.get(row).map(|ft| ft.ticket.as_ref().clone())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TicketId, TicketPriority, TicketType};
    use std::sync::Arc;

    fn make_ticket(id: &str, title: &str, status: TicketStatus) -> TicketMetadata {
        TicketMetadata {
            id: Some(TicketId::new_unchecked(id)),
            title: Some(title.to_string()),
            status: Some(status),
            priority: Some(TicketPriority::P2),
            ticket_type: Some(TicketType::Task),
            ..Default::default()
        }
    }

    fn default_state() -> BoardState {
        BoardState {
            tickets: vec![],
            search_query: String::new(),
            search_focused: false,
            current_column: 0,
            current_row: 0,
            visible_columns: [true; 5],
            column_scroll_offsets: [0; 5],
            is_loading: false,
            init_result: InitResult::Ok,
            toast: None,
            edit_mode: None,
            debounce_ms: 10,
            last_search_change: None,
            search_in_flight: false,
        }
    }

    // ========================================================================
    // Navigation Tests
    // ========================================================================

    #[test]
    fn test_find_next_visible_column_all_visible() {
        let visible = [true; 5];
        assert_eq!(find_next_visible_column(&visible, 0), 1);
        assert_eq!(find_next_visible_column(&visible, 1), 2);
        assert_eq!(find_next_visible_column(&visible, 3), 4);
        // At last column, stays there
        assert_eq!(find_next_visible_column(&visible, 4), 4);
    }

    #[test]
    fn test_find_next_visible_column_skips_hidden() {
        let visible = [true, false, false, true, false];
        assert_eq!(find_next_visible_column(&visible, 0), 3);
        // From 3, nowhere to go
        assert_eq!(find_next_visible_column(&visible, 3), 3);
    }

    #[test]
    fn test_find_prev_visible_column_all_visible() {
        let visible = [true; 5];
        assert_eq!(find_prev_visible_column(&visible, 4), 3);
        assert_eq!(find_prev_visible_column(&visible, 3), 2);
        assert_eq!(find_prev_visible_column(&visible, 1), 0);
        // At first column, stays there
        assert_eq!(find_prev_visible_column(&visible, 0), 0);
    }

    #[test]
    fn test_find_prev_visible_column_skips_hidden() {
        let visible = [true, false, false, true, false];
        assert_eq!(find_prev_visible_column(&visible, 3), 0);
        // From 0, nowhere to go
        assert_eq!(find_prev_visible_column(&visible, 0), 0);
    }

    #[test]
    fn test_find_next_visible_column_none_visible() {
        let visible = [false; 5];
        // Returns current when none visible
        assert_eq!(find_next_visible_column(&visible, 2), 2);
    }

    // ========================================================================
    // Reducer Tests
    // ========================================================================

    // Default column height for tests
    const TEST_COLUMN_HEIGHT: usize = 10;

    #[test]
    fn test_reduce_move_right() {
        let state = BoardState {
            current_column: 0,
            visible_columns: [true; 5],
            ..default_state()
        };
        let new_state = reduce_board_state(state, BoardAction::MoveRight, TEST_COLUMN_HEIGHT);
        assert_eq!(new_state.current_column, 1);
    }

    #[test]
    fn test_reduce_move_left() {
        let state = BoardState {
            current_column: 2,
            visible_columns: [true; 5],
            ..default_state()
        };
        let new_state = reduce_board_state(state, BoardAction::MoveLeft, TEST_COLUMN_HEIGHT);
        assert_eq!(new_state.current_column, 1);
    }

    #[test]
    fn test_reduce_move_up() {
        let state = BoardState {
            current_row: 3,
            ..default_state()
        };
        let new_state = reduce_board_state(state, BoardAction::MoveUp, TEST_COLUMN_HEIGHT);
        assert_eq!(new_state.current_row, 2);
    }

    #[test]
    fn test_reduce_move_up_at_top() {
        let state = BoardState {
            current_row: 0,
            ..default_state()
        };
        let new_state = reduce_board_state(state, BoardAction::MoveUp, TEST_COLUMN_HEIGHT);
        assert_eq!(new_state.current_row, 0);
    }

    #[test]
    fn test_reduce_move_down_with_tickets() {
        let state = BoardState {
            current_row: 0,
            current_column: 0,
            tickets: vec![
                make_ticket("j-1", "Task 1", TicketStatus::New),
                make_ticket("j-2", "Task 2", TicketStatus::New),
                make_ticket("j-3", "Task 3", TicketStatus::New),
            ],
            ..default_state()
        };
        let new_state = reduce_board_state(state, BoardAction::MoveDown, TEST_COLUMN_HEIGHT);
        assert_eq!(new_state.current_row, 1);
    }

    #[test]
    fn test_reduce_toggle_column() {
        let state = BoardState {
            visible_columns: [true; 5],
            ..default_state()
        };
        let new_state = reduce_board_state(state, BoardAction::ToggleColumn(1), TEST_COLUMN_HEIGHT);
        assert!(!new_state.visible_columns[1]);
        assert!(new_state.visible_columns[0]);
        assert!(new_state.visible_columns[2]);
    }

    #[test]
    fn test_reduce_toggle_column_adjusts_selection() {
        let state = BoardState {
            current_column: 1,
            visible_columns: [true; 5],
            ..default_state()
        };
        // Toggle off column 1 (where we are)
        let new_state = reduce_board_state(state, BoardAction::ToggleColumn(1), TEST_COLUMN_HEIGHT);
        assert!(!new_state.visible_columns[1]);
        // Should have moved to first visible column (0)
        assert_eq!(new_state.current_column, 0);
    }

    #[test]
    fn test_reduce_focus_search() {
        let state = BoardState {
            search_focused: false,
            ..default_state()
        };
        let new_state = reduce_board_state(state, BoardAction::FocusSearch, TEST_COLUMN_HEIGHT);
        assert!(new_state.search_focused);
    }

    #[test]
    fn test_reduce_update_search() {
        let state = default_state();
        let new_state = reduce_board_state(
            state,
            BoardAction::UpdateSearch("bug".to_string()),
            TEST_COLUMN_HEIGHT,
        );
        assert_eq!(new_state.search_query, "bug");
    }

    #[test]
    fn test_reduce_exit_search() {
        let state = BoardState {
            search_focused: true,
            search_query: "test".to_string(),
            ..default_state()
        };
        let new_state = reduce_board_state(state, BoardAction::ExitSearch, TEST_COLUMN_HEIGHT);
        assert!(!new_state.search_focused);
        assert_eq!(new_state.search_query, "test"); // Query preserved
    }

    #[test]
    fn test_reduce_clear_search_and_exit() {
        let state = BoardState {
            search_focused: true,
            search_query: "test".to_string(),
            ..default_state()
        };
        let new_state =
            reduce_board_state(state, BoardAction::ClearSearchAndExit, TEST_COLUMN_HEIGHT);
        assert!(!new_state.search_focused);
        assert_eq!(new_state.search_query, ""); // Query cleared
    }

    #[test]
    fn test_reduce_create_new() {
        let state = default_state();
        let new_state = reduce_board_state(state, BoardAction::CreateNew, TEST_COLUMN_HEIGHT);
        assert_eq!(new_state.edit_mode, Some(EditMode::Creating));
    }

    #[test]
    fn test_reduce_cancel_edit() {
        let state = BoardState {
            edit_mode: Some(EditMode::Creating),
            ..default_state()
        };
        let new_state = reduce_board_state(state, BoardAction::CancelEdit, TEST_COLUMN_HEIGHT);
        assert_eq!(new_state.edit_mode, None);
    }

    // ========================================================================
    // View Model Tests
    // ========================================================================

    #[test]
    fn test_compute_view_model_empty() {
        let state = default_state();
        let view_model = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

        assert_eq!(view_model.columns.len(), 5); // All visible
        assert_eq!(view_model.total_all_tickets, 0);
        assert_eq!(view_model.total_filtered_tickets, 0);
        assert!(view_model.selected_ticket.is_none());
        assert_eq!(view_model.empty_state, Some(EmptyStateKind::NoTickets));
    }

    #[test]
    fn test_compute_view_model_with_tickets() {
        let state = BoardState {
            tickets: vec![
                make_ticket("j-1", "Task 1", TicketStatus::New),
                make_ticket("j-2", "Task 2", TicketStatus::New),
                make_ticket("j-3", "Task 3", TicketStatus::InProgress),
            ],
            ..default_state()
        };
        let view_model = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

        assert_eq!(view_model.total_all_tickets, 3);
        assert_eq!(view_model.total_filtered_tickets, 3);
        assert!(view_model.empty_state.is_none());

        // Check column ticket counts
        let new_col = view_model
            .columns
            .iter()
            .find(|c| c.status == TicketStatus::New)
            .unwrap();
        assert_eq!(new_col.ticket_count, 2);

        let wip_col = view_model
            .columns
            .iter()
            .find(|c| c.status == TicketStatus::InProgress)
            .unwrap();
        assert_eq!(wip_col.ticket_count, 1);
    }

    #[test]
    fn test_compute_view_model_with_search() {
        let state = BoardState {
            tickets: vec![
                make_ticket("j-1", "Fix login bug", TicketStatus::New),
                make_ticket("j-2", "Add feature", TicketStatus::New),
                make_ticket("j-3", "Another bug", TicketStatus::InProgress),
            ],
            search_query: "bug".to_string(),
            ..default_state()
        };
        let view_model = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

        assert_eq!(view_model.total_all_tickets, 3);
        assert_eq!(view_model.total_filtered_tickets, 2); // Only "bug" tickets
        assert_eq!(view_model.search.query, "bug");
    }

    #[test]
    fn test_compute_view_model_hidden_columns() {
        let state = BoardState {
            visible_columns: [true, false, false, true, false],
            tickets: vec![make_ticket("j-1", "Task", TicketStatus::New)],
            ..default_state()
        };
        let view_model = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

        // Only 2 columns should be visible
        assert_eq!(view_model.columns.len(), 2);
        assert_eq!(view_model.columns[0].status, TicketStatus::New);
        assert_eq!(view_model.columns[1].status, TicketStatus::Complete);
    }

    #[test]
    fn test_compute_view_model_selected_ticket() {
        let state = BoardState {
            tickets: vec![
                make_ticket("j-1", "Task 1", TicketStatus::New),
                make_ticket("j-2", "Task 2", TicketStatus::New),
            ],
            current_column: 0,
            current_row: 1, // Second ticket
            ..default_state()
        };
        let view_model = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

        let selected = view_model.selected_ticket.unwrap();
        assert_eq!(selected.id.as_deref(), Some("j-2"));
    }

    #[test]
    fn test_compute_view_model_column_toggles_string() {
        let state = BoardState {
            visible_columns: [true, true, false, true, false],
            ..default_state()
        };
        let view_model = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

        assert_eq!(view_model.column_toggles, "[N][X][ ][C][ ]");
    }

    #[test]
    fn test_compute_view_model_loading_state() {
        let state = BoardState {
            is_loading: true,
            ..default_state()
        };
        let view_model = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

        assert_eq!(view_model.empty_state, Some(EmptyStateKind::Loading));
    }

    #[test]
    fn test_compute_view_model_no_janus_dir() {
        let state = BoardState {
            init_result: InitResult::NoJanusDir,
            ..default_state()
        };
        let view_model = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

        assert_eq!(view_model.empty_state, Some(EmptyStateKind::NoJanusDir));
    }

    #[test]
    fn test_compute_view_model_editing_mode() {
        let state = BoardState {
            edit_mode: Some(EditMode::Creating),
            tickets: vec![make_ticket("j-1", "Task", TicketStatus::New)],
            ..default_state()
        };
        let view_model = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

        assert!(view_model.is_editing);
        // Shortcuts should be edit shortcuts when editing
        assert!(view_model.shortcuts.iter().any(|s| s.key == "C-s"));
    }

    // ========================================================================
    // Helper Function Tests
    // ========================================================================

    #[test]
    fn test_get_ticket_at_valid() {
        let state = BoardState {
            tickets: vec![
                make_ticket("j-1", "Task 1", TicketStatus::New),
                make_ticket("j-2", "Task 2", TicketStatus::New),
                make_ticket("j-3", "Task 3", TicketStatus::InProgress),
            ],
            ..default_state()
        };

        let ticket = get_ticket_at(&state, 0, 1).unwrap();
        assert_eq!(ticket.id.as_deref(), Some("j-2"));

        let ticket = get_ticket_at(&state, 2, 0).unwrap();
        assert_eq!(ticket.id.as_deref(), Some("j-3"));
    }

    #[test]
    fn test_get_ticket_at_invalid() {
        let state = BoardState {
            tickets: vec![make_ticket("j-1", "Task 1", TicketStatus::New)],
            ..default_state()
        };

        // Out of bounds column
        assert!(get_ticket_at(&state, 10, 0).is_none());

        // Out of bounds row
        assert!(get_ticket_at(&state, 0, 10).is_none());

        // Empty column
        assert!(get_ticket_at(&state, 1, 0).is_none());
    }

    #[test]
    fn test_get_column_tickets_filters_correctly() {
        let filtered = vec![
            FilteredTicket {
                ticket: Arc::new(make_ticket("j-1", "New task", TicketStatus::New)),
                score: 0,
                title_indices: vec![],
                is_semantic: false,
            },
            FilteredTicket {
                ticket: Arc::new(make_ticket("j-2", "WIP task", TicketStatus::InProgress)),
                score: 0,
                title_indices: vec![],
                is_semantic: false,
            },
            FilteredTicket {
                ticket: Arc::new(make_ticket("j-3", "Another new", TicketStatus::New)),
                score: 0,
                title_indices: vec![],
                is_semantic: false,
            },
        ];

        let new_tickets = get_column_tickets(&filtered, TicketStatus::New);
        assert_eq!(new_tickets.len(), 2);

        let wip_tickets = get_column_tickets(&filtered, TicketStatus::InProgress);
        assert_eq!(wip_tickets.len(), 1);

        let done_tickets = get_column_tickets(&filtered, TicketStatus::Complete);
        assert_eq!(done_tickets.len(), 0);
    }

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
    fn test_scroll_down_keeps_selection_centered() {
        // Start at top of a 15-ticket column, column height = 5
        let mut state = BoardState {
            tickets: (0..15)
                .map(|i| make_ticket(&format!("j-{i}"), "Task", TicketStatus::New))
                .collect(),
            current_column: 0,
            current_row: 4,
            column_scroll_offsets: [0; 5],
            ..default_state()
        };

        // Move down to row 5, should center it
        // half_height = 5/2 = 2, ideal_offset = 5 - 2 = 3
        state = reduce_board_state(state, BoardAction::MoveDown, 5);
        assert_eq!(state.current_row, 5);
        assert_eq!(
            state.column_scroll_offsets[0], 3,
            "Should center selected row (offset = row - half_height)"
        );

        // Move down again to row 6
        // ideal_offset = 6 - 2 = 4
        state = reduce_board_state(state, BoardAction::MoveDown, 5);
        assert_eq!(state.current_row, 6);
        assert_eq!(
            state.column_scroll_offsets[0], 4,
            "Should center selected row"
        );
    }

    #[test]
    fn test_scroll_up_keeps_selection_centered() {
        let mut state = BoardState {
            tickets: (0..15)
                .map(|i| make_ticket(&format!("j-{i}"), "Task", TicketStatus::New))
                .collect(),
            current_column: 0,
            current_row: 5,
            column_scroll_offsets: [5, 0, 0, 0, 0], // Scrolled down
            ..default_state()
        };

        // Move up to row 4, should center it
        // half_height = 5/2 = 2, ideal_offset = 4 - 2 = 2
        state = reduce_board_state(state, BoardAction::MoveUp, 5);
        assert_eq!(state.current_row, 4);
        assert_eq!(
            state.column_scroll_offsets[0], 2,
            "Should center selected row"
        );
    }

    #[test]
    fn test_go_to_top_resets_scroll() {
        let mut state = BoardState {
            tickets: (0..15)
                .map(|i| make_ticket(&format!("j-{i}"), "Task", TicketStatus::New))
                .collect(),
            current_column: 0,
            current_row: 10,
            column_scroll_offsets: [5, 0, 0, 0, 0],
            ..default_state()
        };

        state = reduce_board_state(state, BoardAction::GoToTop, 5);
        assert_eq!(state.current_row, 0);
        assert_eq!(
            state.column_scroll_offsets[0], 0,
            "Scroll should reset to 0"
        );
    }

    #[test]
    fn test_go_to_bottom_adjusts_scroll() {
        let mut state = BoardState {
            tickets: (0..15)
                .map(|i| make_ticket(&format!("j-{i}"), "Task", TicketStatus::New))
                .collect(),
            current_column: 0,
            current_row: 0,
            column_scroll_offsets: [0; 5],
            ..default_state()
        };

        state = reduce_board_state(state, BoardAction::GoToBottom, 5);
        assert_eq!(state.current_row, 14, "Should be at last ticket");
        // With column_height=5 and selected_row=14, scroll so row 14 is at
        // last visible position: 14 - (5-1) = 10
        assert_eq!(
            state.column_scroll_offsets[0], 10,
            "Should scroll to show bottom (14-4=10)"
        );
    }

    #[test]
    fn test_page_down_navigation() {
        let mut state = BoardState {
            tickets: (0..20)
                .map(|i| make_ticket(&format!("j-{i}"), "Task", TicketStatus::New))
                .collect(),
            current_column: 0,
            current_row: 0,
            column_scroll_offsets: [0; 5],
            ..default_state()
        };

        // Page down with height 10 should move 5 (half page)
        state = reduce_board_state(state, BoardAction::PageDown, 10);
        assert_eq!(state.current_row, 5);
    }

    #[test]
    fn test_page_up_navigation() {
        let mut state = BoardState {
            tickets: (0..20)
                .map(|i| make_ticket(&format!("j-{i}"), "Task", TicketStatus::New))
                .collect(),
            current_column: 0,
            current_row: 15,
            column_scroll_offsets: [10, 0, 0, 0, 0],
            ..default_state()
        };

        // Page up with height 10 should move back 5
        state = reduce_board_state(state, BoardAction::PageUp, 10);
        assert_eq!(state.current_row, 10);
    }

    #[test]
    fn test_column_change_preserves_scroll_in_target() {
        // Have scrolled state in column 0, move to column 2
        let mut state =
            BoardState {
                tickets: {
                    let mut tickets: Vec<_> = (0..10)
                        .map(|i| make_ticket(&format!("j-new-{i}"), "Task", TicketStatus::New))
                        .collect();
                    tickets.extend((0..10).map(|i| {
                        make_ticket(&format!("j-wip-{i}"), "Task", TicketStatus::InProgress)
                    }));
                    tickets
                },
                current_column: 0,
                current_row: 8,
                column_scroll_offsets: [4, 0, 0, 0, 0],
                visible_columns: [true; 5],
                ..default_state()
            };

        // Move right to Next column (empty), then right to InProgress
        state = reduce_board_state(state, BoardAction::MoveRight, 5);
        state = reduce_board_state(state, BoardAction::MoveRight, 5);

        // Should be in InProgress column now
        assert_eq!(state.current_column, 2);
        // Row should be adjusted since we can't exceed column's max
        assert!(state.current_row <= 9);
    }

    #[test]
    fn test_view_model_shows_hidden_counts() {
        let state = BoardState {
            tickets: (0..15)
                .map(|i| make_ticket(&format!("j-{i}"), "Task", TicketStatus::New))
                .collect(),
            current_column: 0,
            current_row: 7,
            column_scroll_offsets: [5, 0, 0, 0, 0], // Scrolled down 5
            visible_columns: [true; 5],
            ..default_state()
        };

        let vm = compute_board_view_model(&state, 5);

        let new_column = vm
            .columns
            .iter()
            .find(|c| c.status == TicketStatus::New)
            .unwrap();
        assert_eq!(
            new_column.hidden_above, 5,
            "Should have 5 tickets hidden above"
        );
        assert_eq!(
            new_column.hidden_below, 5,
            "Should have 5 tickets hidden below (15-5-5=5)"
        );
        assert_eq!(
            new_column.visible_row_count, 5,
            "Should show 5 visible cards"
        );
    }
}
