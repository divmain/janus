//! IssueBrowser snapshot and integration tests
//!
//! These tests complement the unit tests in `src/tui/view/model.rs` by testing:
//! - View model computation snapshots
//! - Reducer action sequences
//! - Key-to-action mapping (documented behavior)
//!
//! The unit tests in the model module test individual functions in isolation.
//! These tests focus on integration and edge cases using the test fixtures.

mod common;

use common::mock_data::{mock_ticket, mock_tickets, TicketBuilder};
use janus::tui::repository::InitResult;
use janus::tui::state::Pane;
use janus::tui::view::model::*;
use janus::types::{TicketPriority, TicketStatus, TicketType};

use iocraft::prelude::{KeyCode, KeyModifiers};

// ============================================================================
// View Model Snapshot Tests
// ============================================================================

#[test]
fn test_view_model_empty_state() {
    let state = ViewState {
        is_loading: false,
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_view_model(&state, 20);

    // Snapshot the key properties of empty state
    insta::assert_debug_snapshot!(
        "empty_view",
        (
            &vm.empty_state,
            vm.total_all_tickets,
            vm.list.tickets.len(),
            vm.is_editing,
        )
    );
}

#[test]
fn test_view_model_loading() {
    let state = ViewState {
        is_loading: true,
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_view_model(&state, 20);

    insta::assert_debug_snapshot!(
        "loading_view",
        (&vm.empty_state, vm.is_editing, vm.total_all_tickets)
    );
}

#[test]
fn test_view_model_no_janus_dir() {
    let state = ViewState {
        is_loading: false,
        init_result: InitResult::NoJanusDir,
        ..Default::default()
    };
    let vm = compute_view_model(&state, 20);

    insta::assert_debug_snapshot!("no_janus_dir_view", (&vm.empty_state, vm.total_all_tickets));
}

#[test]
fn test_view_model_with_tickets() {
    let state = ViewState {
        tickets: vec![
            mock_ticket("j-1", TicketStatus::New),
            mock_ticket("j-2", TicketStatus::Next),
            mock_ticket("j-3", TicketStatus::InProgress),
            mock_ticket("j-4", TicketStatus::Complete),
            mock_ticket("j-5", TicketStatus::Cancelled),
        ],
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_view_model(&state, 20);

    insta::assert_debug_snapshot!(
        "view_with_tickets",
        (
            vm.total_all_tickets,
            vm.list.tickets.len(),
            vm.list.selected_index,
            vm.detail.ticket.as_ref().and_then(|t| t.id.clone()),
        )
    );
}

#[test]
fn test_view_model_with_search() {
    let state = ViewState {
        tickets: mock_tickets(&[
            ("j-bug1", TicketStatus::New),
            ("j-feat1", TicketStatus::New),
            ("j-bug2", TicketStatus::InProgress),
        ]),
        search_query: "bug".to_string(),
        active_pane: Pane::Search,
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_view_model(&state, 20);

    insta::assert_debug_snapshot!(
        "view_search",
        (
            &vm.search.query,
            vm.search.is_focused,
            vm.search.result_count,
            vm.list.tickets.len(),
            vm.total_all_tickets,
        )
    );
}

#[test]
fn test_view_model_search_no_results() {
    let state = ViewState {
        tickets: mock_tickets(&[
            ("j-task1", TicketStatus::New),
            ("j-task2", TicketStatus::New),
        ]),
        search_query: "nonexistent".to_string(),
        active_pane: Pane::List,
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_view_model(&state, 20);

    insta::assert_debug_snapshot!(
        "view_search_no_results",
        (&vm.empty_state, vm.list.tickets.len(), vm.total_all_tickets)
    );
}

#[test]
fn test_view_model_selected_ticket() {
    let state = ViewState {
        tickets: vec![
            mock_ticket("j-1", TicketStatus::New),
            mock_ticket("j-2", TicketStatus::New),
            mock_ticket("j-3", TicketStatus::New),
        ],
        selected_index: 1, // Second ticket
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_view_model(&state, 20);

    let selected_id = vm.detail.ticket.as_ref().and_then(|t| t.id.clone());
    insta::assert_debug_snapshot!("selected_ticket", selected_id);
}

#[test]
fn test_view_model_editing_mode() {
    let state = ViewState {
        tickets: vec![mock_ticket("j-1", TicketStatus::New)],
        edit_mode: Some(EditMode::Creating),
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_view_model(&state, 20);

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
fn test_view_model_editing_existing() {
    let state = ViewState {
        tickets: vec![mock_ticket("j-1", TicketStatus::New)],
        edit_mode: Some(EditMode::Editing {
            ticket_id: "j-1".to_string(),
        }),
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_view_model(&state, 20);

    assert!(vm.is_editing);
    // When editing, all panes should lose focus
    assert!(!vm.list.is_focused);
    assert!(!vm.detail.is_focused);
    assert!(!vm.search.is_focused);
}

#[test]
fn test_view_model_pane_focus_states() {
    // Search pane focused
    let state = ViewState {
        tickets: vec![mock_ticket("j-1", TicketStatus::New)],
        active_pane: Pane::Search,
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_view_model(&state, 20);
    insta::assert_debug_snapshot!(
        "search_pane_focused",
        (
            vm.search.is_focused,
            vm.list.is_focused,
            vm.detail.is_focused
        )
    );

    // List pane focused
    let state = ViewState {
        tickets: vec![mock_ticket("j-1", TicketStatus::New)],
        active_pane: Pane::List,
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_view_model(&state, 20);
    insta::assert_debug_snapshot!(
        "list_pane_focused",
        (
            vm.search.is_focused,
            vm.list.is_focused,
            vm.detail.is_focused
        )
    );

    // Detail pane focused
    let state = ViewState {
        tickets: vec![mock_ticket("j-1", TicketStatus::New)],
        active_pane: Pane::Detail,
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_view_model(&state, 20);
    insta::assert_debug_snapshot!(
        "detail_pane_focused",
        (
            vm.search.is_focused,
            vm.list.is_focused,
            vm.detail.is_focused
        )
    );
}

#[test]
fn test_view_model_scroll_state() {
    let state = ViewState {
        tickets: (0..30)
            .map(|i| mock_ticket(&format!("j-{}", i), TicketStatus::New))
            .collect(),
        selected_index: 15,
        scroll_offset: 10,
        init_result: InitResult::Ok,
        ..Default::default()
    };
    let vm = compute_view_model(&state, 10);

    insta::assert_debug_snapshot!(
        "scroll_state",
        (
            vm.list.selected_index,
            vm.list.scroll_offset,
            vm.list.visible_count,
            vm.list.tickets.len()
        )
    );
}

// ============================================================================
// Reducer Action Sequence Tests
// ============================================================================

#[test]
fn test_navigation_sequence() {
    let state = ViewState {
        tickets: (0..10)
            .map(|i| mock_ticket(&format!("j-{}", i), TicketStatus::New))
            .collect(),
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Navigate down 3 times
    let state = reduce_view_state(state, ViewAction::MoveDown, 20);
    let state = reduce_view_state(state, ViewAction::MoveDown, 20);
    let state = reduce_view_state(state, ViewAction::MoveDown, 20);

    insta::assert_debug_snapshot!(
        "nav_down_three",
        (state.selected_index, state.scroll_offset)
    );
}

#[test]
fn test_navigation_with_scroll_adjustment() {
    let state = ViewState {
        tickets: (0..30)
            .map(|i| mock_ticket(&format!("j-{}", i), TicketStatus::New))
            .collect(),
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Navigate to bottom in a small list view (height 5)
    let state = reduce_view_state(state, ViewAction::GoToBottom, 5);

    insta::assert_debug_snapshot!(
        "nav_to_bottom_scroll",
        (state.selected_index, state.scroll_offset)
    );
}

#[test]
fn test_pane_cycle_sequence() {
    let state = ViewState {
        tickets: vec![mock_ticket("j-1", TicketStatus::New)],
        active_pane: Pane::List,
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Cycle through all panes
    let state = reduce_view_state(state, ViewAction::CyclePaneForward, 20);
    let pane1 = state.active_pane;
    let state = reduce_view_state(state, ViewAction::CyclePaneForward, 20);
    let pane2 = state.active_pane;
    let state = reduce_view_state(state, ViewAction::CyclePaneForward, 20);
    let pane3 = state.active_pane;

    insta::assert_debug_snapshot!(
        "pane_cycle_sequence",
        (pane1, pane2, pane3) // Should be Detail, Search, List
    );
}

#[test]
fn test_search_flow() {
    let state = ViewState {
        tickets: (0..10)
            .map(|i| mock_ticket(&format!("j-{}", i), TicketStatus::New))
            .collect(),
        selected_index: 5,
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Enter search mode
    let state = reduce_view_state(state, ViewAction::FocusSearch, 20);
    // Update search query
    let state = reduce_view_state(state, ViewAction::UpdateSearch("test".to_string()), 20);

    insta::assert_debug_snapshot!(
        "search_flow",
        (
            &state.search_query,
            state.active_pane,
            state.selected_index // Should be reset to 0
        )
    );

    // Exit search
    let state = reduce_view_state(state, ViewAction::ExitSearch, 20);
    insta::assert_debug_snapshot!(
        "search_exit",
        (&state.search_query, state.active_pane) // Query preserved
    );
}

#[test]
fn test_search_clear_and_exit() {
    let state = ViewState {
        tickets: vec![mock_ticket("j-1", TicketStatus::New)],
        search_query: "test query".to_string(),
        active_pane: Pane::Search,
        init_result: InitResult::Ok,
        ..Default::default()
    };

    let state = reduce_view_state(state, ViewAction::ClearSearchAndExit, 20);

    assert!(
        state.search_query.is_empty(),
        "Search query should be cleared"
    );
    assert_eq!(state.active_pane, Pane::List);
}

#[test]
fn test_edit_mode_flow() {
    let state = ViewState {
        tickets: vec![mock_ticket("j-1", TicketStatus::New)],
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Enter create mode
    let state = reduce_view_state(state, ViewAction::CreateNew, 20);
    assert_eq!(state.edit_mode, Some(EditMode::Creating));

    // Cancel edit
    let state = reduce_view_state(state, ViewAction::CancelEdit, 20);
    assert_eq!(state.edit_mode, None);
}

#[test]
fn test_vertical_navigation_bounds() {
    let state = ViewState {
        tickets: mock_tickets(&[("j-1", TicketStatus::New), ("j-2", TicketStatus::New)]),
        selected_index: 0,
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Move up at top - should stay at 0
    let state = reduce_view_state(state, ViewAction::MoveUp, 20);
    assert_eq!(state.selected_index, 0);

    // Move down twice - should stop at 1 (max)
    let state = reduce_view_state(state, ViewAction::MoveDown, 20);
    let state = reduce_view_state(state, ViewAction::MoveDown, 20);
    assert_eq!(state.selected_index, 1);
}

#[test]
fn test_page_navigation() {
    let state = ViewState {
        tickets: (0..50)
            .map(|i| mock_ticket(&format!("j-{}", i), TicketStatus::New))
            .collect(),
        selected_index: 25,
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Page down with list height 20 (jump = 10)
    let state = reduce_view_state(state, ViewAction::PageDown, 20);
    assert_eq!(state.selected_index, 35);

    // Page up
    let state = reduce_view_state(state, ViewAction::PageUp, 20);
    assert_eq!(state.selected_index, 25);
}

// ============================================================================
// Key Mapping Tests
// ============================================================================

#[test]
fn test_key_to_action_navigation() {
    // Vim-style keys
    assert_eq!(
        key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, Pane::List),
        Some(ViewAction::MoveDown)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('k'), KeyModifiers::NONE, Pane::List),
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

    // Arrow keys
    assert_eq!(
        key_to_action(KeyCode::Down, KeyModifiers::NONE, Pane::List),
        Some(ViewAction::MoveDown)
    );
    assert_eq!(
        key_to_action(KeyCode::Up, KeyModifiers::NONE, Pane::List),
        Some(ViewAction::MoveUp)
    );

    // Page keys
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
fn test_key_to_action_search_mode() {
    // Escape clears and exits
    assert_eq!(
        key_to_action(KeyCode::Esc, KeyModifiers::NONE, Pane::Search),
        Some(ViewAction::ClearSearchAndExit)
    );

    // Enter/Tab exit keeping query
    assert_eq!(
        key_to_action(KeyCode::Enter, KeyModifiers::NONE, Pane::Search),
        Some(ViewAction::ExitSearch)
    );
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
fn test_key_to_action_search_mode_passthrough() {
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
fn test_key_to_action_detail_escape() {
    // Escape in detail pane cycles back
    assert_eq!(
        key_to_action(KeyCode::Esc, KeyModifiers::NONE, Pane::Detail),
        Some(ViewAction::CyclePaneBackward)
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
    assert_eq!(
        key_to_action(KeyCode::Home, KeyModifiers::NONE, Pane::List),
        None
    );
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_view_with_rich_ticket_data() {
    // Test with fully populated ticket metadata
    let ticket = TicketBuilder::new("j-rich1")
        .title("Important bug fix")
        .status(TicketStatus::InProgress)
        .ticket_type(TicketType::Bug)
        .priority(TicketPriority::P0)
        .dep("j-dep1")
        .parent("j-parent")
        .build();

    let state = ViewState {
        tickets: vec![ticket],
        init_result: InitResult::Ok,
        ..Default::default()
    };

    let vm = compute_view_model(&state, 20);

    // Verify the ticket is selected
    assert_eq!(
        vm.detail.ticket.as_ref().and_then(|t| t.id.clone()),
        Some("j-rich1".to_string())
    );
    assert_eq!(vm.list.tickets.len(), 1);
}

#[test]
fn test_get_ticket_at_helper() {
    let state = ViewState {
        tickets: mock_tickets(&[
            ("j-1", TicketStatus::New),
            ("j-2", TicketStatus::New),
            ("j-3", TicketStatus::InProgress),
        ]),
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // Get first ticket
    let ticket = get_ticket_at(&state, 0);
    assert!(ticket.is_some());
    assert_eq!(ticket.unwrap().id, Some("j-1".to_string()));

    // Get second ticket
    let ticket = get_ticket_at(&state, 1);
    assert!(ticket.is_some());
    assert_eq!(ticket.unwrap().id, Some("j-2".to_string()));

    // Out of bounds returns None
    assert!(get_ticket_at(&state, 10).is_none());
}

#[test]
fn test_get_selected_ticket_helper() {
    let state = ViewState {
        tickets: mock_tickets(&[
            ("j-1", TicketStatus::New),
            ("j-2", TicketStatus::New),
            ("j-3", TicketStatus::InProgress),
        ]),
        selected_index: 1,
        init_result: InitResult::Ok,
        ..Default::default()
    };

    let ticket = get_selected_ticket(&state);
    assert!(ticket.is_some());
    assert_eq!(ticket.unwrap().id, Some("j-2".to_string()));
}

#[test]
fn test_adjust_scroll_edge_cases() {
    // Within bounds - no change
    assert_eq!(adjust_scroll(0, 5, 10), 0);
    assert_eq!(adjust_scroll(5, 8, 10), 5);

    // Selected below visible - scroll down
    assert_eq!(adjust_scroll(0, 15, 10), 6);

    // Selected above visible - scroll up
    assert_eq!(adjust_scroll(10, 5, 10), 5);

    // Zero height
    assert_eq!(adjust_scroll(5, 10, 0), 0);
}

#[test]
fn test_complex_user_session() {
    // Simulate a realistic user session
    let state = ViewState {
        tickets: (0..20)
            .map(|i| mock_ticket(&format!("j-{}", i), TicketStatus::New))
            .collect(),
        init_result: InitResult::Ok,
        ..Default::default()
    };

    // User navigates: down 5x, search, type query, exit search, tab to detail, escape back
    let state = reduce_view_state(state, ViewAction::MoveDown, 10);
    let state = reduce_view_state(state, ViewAction::MoveDown, 10);
    let state = reduce_view_state(state, ViewAction::MoveDown, 10);
    let state = reduce_view_state(state, ViewAction::MoveDown, 10);
    let state = reduce_view_state(state, ViewAction::MoveDown, 10);
    assert_eq!(state.selected_index, 5);

    let state = reduce_view_state(state, ViewAction::FocusSearch, 10);
    assert_eq!(state.active_pane, Pane::Search);

    let state = reduce_view_state(state, ViewAction::UpdateSearch("test".to_string()), 10);
    assert_eq!(state.search_query, "test");
    assert_eq!(state.selected_index, 0); // Reset on search

    let state = reduce_view_state(state, ViewAction::ExitSearch, 10);
    assert_eq!(state.active_pane, Pane::List);
    assert_eq!(state.search_query, "test"); // Query preserved

    let state = reduce_view_state(state, ViewAction::CyclePaneForward, 10);
    assert_eq!(state.active_pane, Pane::Detail);

    let state = reduce_view_state(state, ViewAction::CyclePaneBackward, 10);
    assert_eq!(state.active_pane, Pane::List);
}
