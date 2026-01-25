//! KanbanBoard snapshot and integration tests
//!
//! These tests complement the 31 unit tests in `src/tui/board/model.rs` by testing:
//! - View model computation snapshots
//! - Reducer action sequences
//! - Key-to-action mapping (documented behavior)
//!
//! The unit tests in the model module test individual functions in isolation.
//! These tests focus on integration and edge cases using the test fixtures.

mod common;

use common::mock_data::{mock_tickets, TicketBuilder};
use janus::tui::board::model::*;
use janus::tui::repository::InitResult;
use janus::types::{TicketPriority, TicketStatus, TicketType};

// Default column height for tests
const TEST_COLUMN_HEIGHT: usize = 10;

// ============================================================================
// View Model Snapshot Tests
// ============================================================================
// DELETED: All view model snapshot tests removed per section 1.4 of TEST_REVIEW.md

// ============================================================================
// Reducer Action Sequence Tests
// ============================================================================

#[test]
fn test_navigation_action_sequence() {
    let state = BoardState {
        tickets: mock_tickets(&[
            ("j-1", TicketStatus::New),
            ("j-2", TicketStatus::New),
            ("j-3", TicketStatus::Next),
        ]),
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Navigate right twice
    let state = reduce_board_state(state, BoardAction::MoveRight, TEST_COLUMN_HEIGHT);
    let state = reduce_board_state(state, BoardAction::MoveRight, TEST_COLUMN_HEIGHT);

    insta::assert_debug_snapshot!("nav_right_twice", (state.current_column, state.current_row));
}

#[test]
fn test_navigation_sequence_with_row_adjustment() {
    // Start with multiple tickets in column 0, navigate to empty column
    let state = BoardState {
        tickets: mock_tickets(&[
            ("j-1", TicketStatus::New),
            ("j-2", TicketStatus::New),
            ("j-3", TicketStatus::New),
        ]),
        current_column: 0,
        current_row: 2, // Third ticket in column 0
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Navigate right to column 1 (Next), which has no tickets
    let state = reduce_board_state(state, BoardAction::MoveRight, TEST_COLUMN_HEIGHT);

    // Row should be adjusted to 0 (max for empty column)
    insta::assert_debug_snapshot!(
        "nav_with_row_adjustment",
        (state.current_column, state.current_row)
    );
}

#[test]
fn test_column_toggle_sequence() {
    let state = BoardState {
        current_column: 1,
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Hide current column
    let state = reduce_board_state(state, BoardAction::ToggleColumn(1), TEST_COLUMN_HEIGHT);

    insta::assert_debug_snapshot!(
        "toggle_current_column",
        (state.visible_columns, state.current_column)
    );
}

#[test]
fn test_toggle_multiple_columns() {
    let state = BoardState {
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Toggle off columns 1, 2, 3
    let state = reduce_board_state(state, BoardAction::ToggleColumn(1), TEST_COLUMN_HEIGHT);
    let state = reduce_board_state(state, BoardAction::ToggleColumn(2), TEST_COLUMN_HEIGHT);
    let state = reduce_board_state(state, BoardAction::ToggleColumn(3), TEST_COLUMN_HEIGHT);

    insta::assert_debug_snapshot!("toggle_multiple_columns", state.visible_columns);
}

#[test]
fn test_toggle_column_back_on() {
    let state = BoardState {
        visible_columns: [true, false, true, true, true],
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Toggle column 1 back on
    let state = reduce_board_state(state, BoardAction::ToggleColumn(1), TEST_COLUMN_HEIGHT);

    assert!(
        state.visible_columns[1],
        "Column 1 should be visible after toggling"
    );
}

#[test]
fn test_search_flow() {
    let state = BoardState {
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Enter search mode
    let state = reduce_board_state(state, BoardAction::FocusSearch, TEST_COLUMN_HEIGHT);
    let state = reduce_board_state(
        state,
        BoardAction::UpdateSearch("test".to_string()),
        TEST_COLUMN_HEIGHT,
    );

    insta::assert_debug_snapshot!("search_active", (&state.search_query, state.search_focused));

    // Exit search
    let state = reduce_board_state(state, BoardAction::ExitSearch, TEST_COLUMN_HEIGHT);
    insta::assert_debug_snapshot!("search_exited", (&state.search_query, state.search_focused));
}

#[test]
fn test_search_clear_and_exit() {
    let state = BoardState {
        search_query: "test query".to_string(),
        search_focused: true,
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Clear and exit
    let state = reduce_board_state(state, BoardAction::ClearSearchAndExit, TEST_COLUMN_HEIGHT);

    assert!(
        state.search_query.is_empty(),
        "Search query should be cleared"
    );
    assert!(!state.search_focused, "Search should not be focused");
}

#[test]
fn test_edit_mode_flow() {
    let state = BoardState {
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Enter create mode
    let state = reduce_board_state(state, BoardAction::CreateNew, TEST_COLUMN_HEIGHT);
    assert_eq!(state.edit_mode, Some(EditMode::Creating));

    // Cancel edit
    let state = reduce_board_state(state, BoardAction::CancelEdit, TEST_COLUMN_HEIGHT);
    assert_eq!(state.edit_mode, None);
}

#[test]
fn test_vertical_navigation_bounds() {
    let state = BoardState {
        tickets: mock_tickets(&[("j-1", TicketStatus::New), ("j-2", TicketStatus::New)]),
        current_column: 0,
        current_row: 0,
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Move up at top - should stay at 0
    let state = reduce_board_state(state, BoardAction::MoveUp, TEST_COLUMN_HEIGHT);
    assert_eq!(state.current_row, 0);

    // Move down twice - should stop at 1 (max)
    let state = reduce_board_state(state, BoardAction::MoveDown, TEST_COLUMN_HEIGHT);
    let state = reduce_board_state(state, BoardAction::MoveDown, TEST_COLUMN_HEIGHT);
    assert_eq!(state.current_row, 1);

    // Move down again - should stay at 1
    let state = reduce_board_state(state, BoardAction::MoveDown, TEST_COLUMN_HEIGHT);
    assert_eq!(state.current_row, 1);
}

#[test]
fn test_horizontal_navigation_bounds() {
    let state = BoardState {
        current_column: 0,
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Move left at leftmost - should stay at 0
    let state = reduce_board_state(state, BoardAction::MoveLeft, TEST_COLUMN_HEIGHT);
    assert_eq!(state.current_column, 0);

    // Move to rightmost
    let state = reduce_board_state(state, BoardAction::MoveRight, TEST_COLUMN_HEIGHT);
    let state = reduce_board_state(state, BoardAction::MoveRight, TEST_COLUMN_HEIGHT);
    let state = reduce_board_state(state, BoardAction::MoveRight, TEST_COLUMN_HEIGHT);
    let state = reduce_board_state(state, BoardAction::MoveRight, TEST_COLUMN_HEIGHT);
    assert_eq!(state.current_column, 4);

    // Move right at rightmost - should stay at 4
    let state = reduce_board_state(state, BoardAction::MoveRight, TEST_COLUMN_HEIGHT);
    assert_eq!(state.current_column, 4);
}

#[test]
fn test_navigation_skips_hidden_columns() {
    let state = BoardState {
        current_column: 0,
        visible_columns: [true, false, false, true, false], // Only 0 and 3 visible
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Move right should skip to column 3
    let state = reduce_board_state(state, BoardAction::MoveRight, TEST_COLUMN_HEIGHT);
    assert_eq!(state.current_column, 3);

    // Move right again should stay at 3 (rightmost visible)
    let state = reduce_board_state(state, BoardAction::MoveRight, TEST_COLUMN_HEIGHT);
    assert_eq!(state.current_column, 3);

    // Move left should go back to 0
    let state = reduce_board_state(state, BoardAction::MoveLeft, TEST_COLUMN_HEIGHT);
    assert_eq!(state.current_column, 0);
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_board_with_rich_ticket_data() {
    // Test with fully populated ticket metadata
    let ticket = TicketBuilder::new("j-rich1")
        .title("Important bug fix")
        .status(TicketStatus::InProgress)
        .ticket_type(TicketType::Bug)
        .priority(TicketPriority::P0)
        .dep("j-dep1")
        .parent("j-parent")
        .build();

    let state = BoardState {
        tickets: vec![ticket],
        current_column: 2, // InProgress
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };

    let vm = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

    // Verify the ticket is in the right column and selected
    let wip_column = vm
        .columns
        .iter()
        .find(|c| c.status == TicketStatus::InProgress)
        .unwrap();
    assert_eq!(wip_column.ticket_count, 1);
    assert!(wip_column.is_active);

    // Verify selected ticket has the right ID
    assert_eq!(
        vm.selected_ticket.as_ref().and_then(|t| t.id.clone()),
        Some("j-rich1".to_string())
    );
}

#[test]
fn test_get_ticket_at_helper() {
    let state = BoardState {
        tickets: mock_tickets(&[
            ("j-1", TicketStatus::New),
            ("j-2", TicketStatus::New),
            ("j-3", TicketStatus::InProgress),
        ]),
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Get first ticket in New column
    let ticket = get_ticket_at(&state, 0, 0);
    assert!(ticket.is_some());
    assert_eq!(ticket.unwrap().id, Some("j-1".to_string()));

    // Get second ticket in New column
    let ticket = get_ticket_at(&state, 0, 1);
    assert!(ticket.is_some());
    assert_eq!(ticket.unwrap().id, Some("j-2".to_string()));

    // Get ticket in InProgress column (index 2)
    let ticket = get_ticket_at(&state, 2, 0);
    assert!(ticket.is_some());
    assert_eq!(ticket.unwrap().id, Some("j-3".to_string()));

    // Out of bounds returns None
    assert!(get_ticket_at(&state, 0, 10).is_none());
    assert!(get_ticket_at(&state, 10, 0).is_none());
    assert!(get_ticket_at(&state, 1, 0).is_none()); // Next column is empty
}

#[test]
fn test_find_next_visible_column_edge_cases() {
    // All visible - standard navigation
    let visible = [true; 5];
    assert_eq!(find_next_visible_column(&visible, 0), 1);
    assert_eq!(find_next_visible_column(&visible, 4), 4); // Stay at end

    // Only first visible
    let visible = [true, false, false, false, false];
    assert_eq!(find_next_visible_column(&visible, 0), 0);

    // Only last visible
    let visible = [false, false, false, false, true];
    assert_eq!(find_next_visible_column(&visible, 4), 4);

    // None visible - should stay in place
    let visible = [false; 5];
    assert_eq!(find_next_visible_column(&visible, 2), 2);
}

#[test]
fn test_find_prev_visible_column_edge_cases() {
    // All visible - standard navigation
    let visible = [true; 5];
    assert_eq!(find_prev_visible_column(&visible, 4), 3);
    assert_eq!(find_prev_visible_column(&visible, 0), 0); // Stay at start

    // Only first visible
    let visible = [true, false, false, false, false];
    assert_eq!(find_prev_visible_column(&visible, 0), 0);

    // Only last visible
    let visible = [false, false, false, false, true];
    assert_eq!(find_prev_visible_column(&visible, 4), 4);

    // Sparse visibility
    let visible = [true, false, true, false, true];
    assert_eq!(find_prev_visible_column(&visible, 4), 2);
    assert_eq!(find_prev_visible_column(&visible, 2), 0);
}

#[test]
fn test_toggle_out_of_bounds_column() {
    let state = BoardState {
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Toggle column 10 (out of bounds) - should do nothing
    let state = reduce_board_state(state, BoardAction::ToggleColumn(10), TEST_COLUMN_HEIGHT);
    assert_eq!(state.visible_columns, [true; 5]);
}

#[test]
fn test_complex_navigation_scenario() {
    // Simulate a realistic user session
    let state = BoardState {
        tickets: mock_tickets(&[
            ("j-1", TicketStatus::New),
            ("j-2", TicketStatus::New),
            ("j-3", TicketStatus::Next),
            ("j-4", TicketStatus::InProgress),
            ("j-5", TicketStatus::Complete),
        ]),
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // User navigates: down, right, down, search, exit search, left
    let state = reduce_board_state(state, BoardAction::MoveDown, TEST_COLUMN_HEIGHT);
    assert_eq!(state.current_row, 1);

    let state = reduce_board_state(state, BoardAction::MoveRight, TEST_COLUMN_HEIGHT);
    assert_eq!(state.current_column, 1);
    // Row should be adjusted since column 1 has only 1 ticket
    assert_eq!(state.current_row, 0);

    let state = reduce_board_state(state, BoardAction::FocusSearch, TEST_COLUMN_HEIGHT);
    assert!(state.search_focused);

    let state = reduce_board_state(state, BoardAction::ExitSearch, TEST_COLUMN_HEIGHT);
    assert!(!state.search_focused);

    let state = reduce_board_state(state, BoardAction::MoveLeft, TEST_COLUMN_HEIGHT);
    assert_eq!(state.current_column, 0);
}
