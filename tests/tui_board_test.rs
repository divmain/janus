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

use common::mock_data::{TicketBuilder, mock_ticket, mock_tickets};
use janus::tui::board::handlers::key_to_action;
use janus::tui::board::model::*;
use janus::tui::repository::InitResult;
use janus::types::{TicketPriority, TicketStatus, TicketType};

use iocraft::prelude::{KeyCode, KeyModifiers};

// Default column height for tests
const TEST_COLUMN_HEIGHT: usize = 10;

// ============================================================================
// View Model Snapshot Tests
// ============================================================================

#[test]
fn test_board_view_model_empty_state() {
    let state = BoardState {
        is_loading: false,
        init_result: InitResult::Ok,
        visible_columns: [true; 5],
        ..Default::default()
    };
    let vm = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

    // Snapshot the key properties of empty state
    insta::assert_debug_snapshot!(
        "empty_board",
        (
            &vm.empty_state,
            vm.total_all_tickets,
            vm.total_filtered_tickets,
            vm.is_editing,
            vm.columns.len()
        )
    );
}

#[test]
fn test_board_view_model_loading() {
    let state = BoardState {
        is_loading: true,
        init_result: InitResult::Ok,
        visible_columns: [true; 5],
        ..Default::default()
    };
    let vm = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

    insta::assert_debug_snapshot!(
        "loading_board",
        (&vm.empty_state, vm.is_editing, vm.total_all_tickets)
    );
}

#[test]
fn test_board_view_model_no_janus_dir() {
    let state = BoardState {
        is_loading: false,
        init_result: InitResult::NoJanusDir,
        visible_columns: [true; 5],
        ..Default::default()
    };
    let vm = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

    insta::assert_debug_snapshot!("no_janus_dir", (&vm.empty_state, vm.total_all_tickets));
}

#[test]
fn test_board_view_model_with_tickets_in_each_column() {
    let state = BoardState {
        tickets: vec![
            mock_ticket("j-1", TicketStatus::New),
            mock_ticket("j-2", TicketStatus::Next),
            mock_ticket("j-3", TicketStatus::InProgress),
            mock_ticket("j-4", TicketStatus::Complete),
            mock_ticket("j-5", TicketStatus::Cancelled),
        ],
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

    // Snapshot column info: (name, ticket_count, is_active)
    let column_info: Vec<_> = vm
        .columns
        .iter()
        .map(|c| (c.name, c.ticket_count, c.is_active))
        .collect();
    insta::assert_debug_snapshot!("board_with_tickets", column_info);
}

#[test]
fn test_board_view_model_with_multiple_tickets_per_column() {
    let state = BoardState {
        tickets: vec![
            mock_ticket("j-1", TicketStatus::New),
            mock_ticket("j-2", TicketStatus::New),
            mock_ticket("j-3", TicketStatus::New),
            mock_ticket("j-4", TicketStatus::InProgress),
            mock_ticket("j-5", TicketStatus::InProgress),
            mock_ticket("j-6", TicketStatus::Complete),
        ],
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

    let column_counts: Vec<_> = vm
        .columns
        .iter()
        .map(|c| (c.name, c.ticket_count))
        .collect();
    insta::assert_debug_snapshot!("multiple_tickets_per_column", column_counts);
}

#[test]
fn test_board_view_model_with_search() {
    let state = BoardState {
        tickets: mock_tickets(&[
            ("j-bug1", TicketStatus::New),
            ("j-feat1", TicketStatus::New),
            ("j-bug2", TicketStatus::InProgress),
        ]),
        search_query: "bug".to_string(),
        search_focused: true,
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

    insta::assert_debug_snapshot!(
        "board_search",
        (
            &vm.search.query,
            vm.search.is_focused,
            vm.search.result_count,
            vm.total_filtered_tickets,
            vm.total_all_tickets
        )
    );
}

#[test]
fn test_board_view_model_search_no_results() {
    let state = BoardState {
        tickets: mock_tickets(&[
            ("j-task1", TicketStatus::New),
            ("j-task2", TicketStatus::New),
        ]),
        search_query: "nonexistent".to_string(),
        search_focused: false,
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

    insta::assert_debug_snapshot!(
        "board_search_no_results",
        (
            &vm.empty_state,
            vm.total_filtered_tickets,
            vm.total_all_tickets
        )
    );
}

#[test]
fn test_board_view_model_hidden_columns() {
    let state = BoardState {
        tickets: vec![
            mock_ticket("j-1", TicketStatus::New),
            mock_ticket("j-2", TicketStatus::Complete),
        ],
        visible_columns: [true, false, false, true, false], // Only New and Complete
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

    let visible_columns: Vec<_> = vm.columns.iter().map(|c| c.name).collect();
    insta::assert_debug_snapshot!("hidden_columns", (visible_columns, &vm.column_toggles));
}

#[test]
fn test_board_view_model_all_columns_hidden() {
    let state = BoardState {
        tickets: vec![mock_ticket("j-1", TicketStatus::New)],
        visible_columns: [false; 5], // All hidden
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

    insta::assert_debug_snapshot!("all_columns_hidden", (vm.columns.len(), &vm.column_toggles));
}

#[test]
fn test_board_view_model_column_toggles_string() {
    let test_cases = [
        ([true, true, true, true, true], "[N][X][I][C][_]"),
        ([false, false, false, false, false], "[ ][ ][ ][ ][ ]"),
        ([true, false, true, false, true], "[N][ ][I][ ][_]"),
        ([false, true, false, true, false], "[ ][X][ ][C][ ]"),
    ];

    for (visible, expected) in test_cases {
        let state = BoardState {
            visible_columns: visible,
            init_result: InitResult::Ok,
            ..Default::default()
        };
        let vm = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);
        assert_eq!(
            vm.column_toggles, expected,
            "Failed for visible_columns: {:?}",
            visible
        );
    }
}

#[test]
fn test_board_view_model_selected_ticket() {
    let state = BoardState {
        tickets: vec![
            mock_ticket("j-1", TicketStatus::New),
            mock_ticket("j-2", TicketStatus::New),
            mock_ticket("j-3", TicketStatus::New),
        ],
        current_column: 0,
        current_row: 1, // Second ticket
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

    // Check selected ticket is j-2
    let selected_id = vm.selected_ticket.as_ref().and_then(|t| t.id.clone());
    insta::assert_debug_snapshot!("selected_ticket", selected_id);
}

#[test]
fn test_board_view_model_editing_mode() {
    let state = BoardState {
        tickets: vec![mock_ticket("j-1", TicketStatus::New)],
        edit_mode: Some(EditMode::Creating),
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

    insta::assert_debug_snapshot!(
        "editing_mode",
        (
            vm.is_editing,
            vm.shortcuts
                .iter()
                .map(|s| s.key.clone())
                .collect::<Vec<_>>()
        )
    );
}

#[test]
fn test_board_view_model_editing_existing() {
    let state = BoardState {
        tickets: vec![mock_ticket("j-1", TicketStatus::New)],
        edit_mode: Some(EditMode::Editing {
            ticket_id: "j-1".to_string(),
        }),
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

    assert!(vm.is_editing);
}

#[test]
fn test_board_view_model_active_column_selection() {
    // When column 2 is selected, only column 2 should be active
    let state = BoardState {
        tickets: mock_tickets(&[
            ("j-1", TicketStatus::New),
            ("j-2", TicketStatus::Next),
            ("j-3", TicketStatus::InProgress),
        ]),
        current_column: 2, // InProgress column
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

    let active_columns: Vec<_> = vm
        .columns
        .iter()
        .filter(|c| c.is_active)
        .map(|c| c.name)
        .collect();
    insta::assert_debug_snapshot!("active_column", active_columns);
}

#[test]
fn test_board_view_model_search_focused_no_active_column() {
    // When search is focused, no column should be active
    let state = BoardState {
        tickets: mock_tickets(&[("j-1", TicketStatus::New)]),
        current_column: 0,
        search_focused: true,
        visible_columns: [true; 5],
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_board_view_model(&state, TEST_COLUMN_HEIGHT);

    let active_columns: Vec<_> = vm
        .columns
        .iter()
        .filter(|c| c.is_active)
        .map(|c| c.name)
        .collect();
    assert!(
        active_columns.is_empty(),
        "No column should be active when search is focused"
    );
}

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
// Key Mapping Tests
// ============================================================================

#[test]
fn test_key_to_action_navigation() {
    // Test vim-style keys
    assert_eq!(
        key_to_action(KeyCode::Char('h'), KeyModifiers::NONE, false),
        Some(BoardAction::MoveLeft)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('l'), KeyModifiers::NONE, false),
        Some(BoardAction::MoveRight)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, false),
        Some(BoardAction::MoveDown)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('k'), KeyModifiers::NONE, false),
        Some(BoardAction::MoveUp)
    );

    // Test arrow keys
    assert_eq!(
        key_to_action(KeyCode::Left, KeyModifiers::NONE, false),
        Some(BoardAction::MoveLeft)
    );
    assert_eq!(
        key_to_action(KeyCode::Right, KeyModifiers::NONE, false),
        Some(BoardAction::MoveRight)
    );
    assert_eq!(
        key_to_action(KeyCode::Down, KeyModifiers::NONE, false),
        Some(BoardAction::MoveDown)
    );
    assert_eq!(
        key_to_action(KeyCode::Up, KeyModifiers::NONE, false),
        Some(BoardAction::MoveUp)
    );
}

#[test]
fn test_key_to_action_column_toggles() {
    assert_eq!(
        key_to_action(KeyCode::Char('1'), KeyModifiers::NONE, false),
        Some(BoardAction::ToggleColumn(0))
    );
    assert_eq!(
        key_to_action(KeyCode::Char('2'), KeyModifiers::NONE, false),
        Some(BoardAction::ToggleColumn(1))
    );
    assert_eq!(
        key_to_action(KeyCode::Char('3'), KeyModifiers::NONE, false),
        Some(BoardAction::ToggleColumn(2))
    );
    assert_eq!(
        key_to_action(KeyCode::Char('4'), KeyModifiers::NONE, false),
        Some(BoardAction::ToggleColumn(3))
    );
    assert_eq!(
        key_to_action(KeyCode::Char('5'), KeyModifiers::NONE, false),
        Some(BoardAction::ToggleColumn(4))
    );
}

#[test]
fn test_key_to_action_app_commands() {
    assert_eq!(
        key_to_action(KeyCode::Char('q'), KeyModifiers::NONE, false),
        Some(BoardAction::Quit)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('/'), KeyModifiers::NONE, false),
        Some(BoardAction::FocusSearch)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('e'), KeyModifiers::NONE, false),
        Some(BoardAction::EditSelected)
    );
    assert_eq!(
        key_to_action(KeyCode::Enter, KeyModifiers::NONE, false),
        Some(BoardAction::EditSelected)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('n'), KeyModifiers::NONE, false),
        Some(BoardAction::CreateNew)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('r'), KeyModifiers::NONE, false),
        Some(BoardAction::Reload)
    );
}

#[test]
fn test_key_to_action_status_changes() {
    assert_eq!(
        key_to_action(KeyCode::Char('s'), KeyModifiers::NONE, false),
        Some(BoardAction::MoveTicketStatusRight)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('S'), KeyModifiers::NONE, false),
        Some(BoardAction::MoveTicketStatusLeft)
    );
}

#[test]
fn test_key_to_action_search_mode_different() {
    // In search mode, navigation keys should not work (return None)
    assert_eq!(
        key_to_action(KeyCode::Char('h'), KeyModifiers::NONE, true),
        None
    );
    assert_eq!(
        key_to_action(KeyCode::Char('l'), KeyModifiers::NONE, true),
        None
    );
    assert_eq!(
        key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, true),
        None
    );
    assert_eq!(
        key_to_action(KeyCode::Char('k'), KeyModifiers::NONE, true),
        None
    );

    // Escape should clear search and exit
    assert_eq!(
        key_to_action(KeyCode::Esc, KeyModifiers::NONE, true),
        Some(BoardAction::ClearSearchAndExit)
    );

    // Enter should exit search (keeping query)
    assert_eq!(
        key_to_action(KeyCode::Enter, KeyModifiers::NONE, true),
        Some(BoardAction::ExitSearch)
    );

    // Tab should also exit search
    assert_eq!(
        key_to_action(KeyCode::Tab, KeyModifiers::NONE, true),
        Some(BoardAction::ExitSearch)
    );
}

#[test]
fn test_key_to_action_search_mode_ctrl_q() {
    // Ctrl+Q should quit even in search mode
    assert_eq!(
        key_to_action(KeyCode::Char('q'), KeyModifiers::CONTROL, true),
        Some(BoardAction::Quit)
    );
}

#[test]
fn test_key_to_action_unknown_keys() {
    // Keys that don't have mappings should return None
    assert_eq!(
        key_to_action(KeyCode::Char('x'), KeyModifiers::NONE, false),
        None
    );
    assert_eq!(
        key_to_action(KeyCode::F(1), KeyModifiers::NONE, false),
        None
    );
    assert_eq!(
        key_to_action(KeyCode::Home, KeyModifiers::NONE, false),
        None
    );
}

#[test]
fn test_key_to_action_scroll_navigation() {
    assert_eq!(
        key_to_action(KeyCode::Char('g'), KeyModifiers::NONE, false),
        Some(BoardAction::GoToTop)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('G'), KeyModifiers::NONE, false),
        Some(BoardAction::GoToBottom)
    );
    assert_eq!(
        key_to_action(KeyCode::PageDown, KeyModifiers::NONE, false),
        Some(BoardAction::PageDown)
    );
    assert_eq!(
        key_to_action(KeyCode::PageUp, KeyModifiers::NONE, false),
        Some(BoardAction::PageUp)
    );
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
