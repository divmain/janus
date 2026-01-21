//! RemoteTui snapshot and integration tests
//!
//! These tests complement the unit tests in `src/tui/remote/model.rs` by testing:
//! - View model computation snapshots
//! - Reducer action sequences
//! - Key-to-action mapping (documented behavior)
//! - Fixture loading (linked_tickets)
//!
//! The unit tests in the model module test individual functions in isolation.
//! These tests focus on integration and edge cases using the test fixtures.

mod common;

use common::fixtures::FixtureGuard;
use common::mock_data::{
    RemoteIssueBuilder, TicketBuilder, mock_linked_ticket, mock_remote_issue, mock_ticket,
};
use janus::remote::RemoteStatus;
use janus::tui::remote::model::*;
use janus::tui::remote::{FilterState, LinkModeState, ViewMode};
use janus::types::{TicketPriority, TicketStatus, TicketType};
use serial_test::serial;

use iocraft::prelude::{KeyCode, KeyModifiers};

// ============================================================================
// Test Helpers
// ============================================================================

fn default_state() -> RemoteState {
    RemoteState::default()
}

fn state_with_data() -> RemoteState {
    RemoteState {
        local_tickets: vec![
            mock_ticket("j-1", TicketStatus::New),
            mock_ticket("j-2", TicketStatus::InProgress),
            mock_ticket("j-3", TicketStatus::Complete),
        ],
        remote_issues: vec![
            mock_remote_issue("GH-1", RemoteStatus::Open),
            mock_remote_issue("GH-2", RemoteStatus::Open),
            mock_remote_issue("GH-3", RemoteStatus::Closed),
        ],
        ..default_state()
    }
}

fn state_with_many_items(local_count: usize, remote_count: usize) -> RemoteState {
    let local_tickets: Vec<_> = (0..local_count)
        .map(|i| mock_ticket(&format!("j-{}", i), TicketStatus::New))
        .collect();
    let remote_issues: Vec<_> = (0..remote_count)
        .map(|i| mock_remote_issue(&format!("GH-{}", i), RemoteStatus::Open))
        .collect();
    RemoteState {
        local_tickets,
        remote_issues,
        ..default_state()
    }
}

// ============================================================================
// View Model Snapshot Tests
// ============================================================================

#[test]
fn test_view_model_empty_state() {
    let state = default_state();
    let vm = compute_remote_view_model(&state, 20);

    // Snapshot the key properties of empty state
    insta::assert_debug_snapshot!(
        "remote_empty_view",
        (
            vm.local_list.tickets.len(),
            vm.remote_list.issues.len(),
            vm.local_list.is_focused,
            vm.is_loading,
        )
    );
}

#[test]
fn test_view_model_with_data() {
    let state = state_with_data();
    let vm = compute_remote_view_model(&state, 20);

    insta::assert_debug_snapshot!(
        "remote_with_data",
        (
            vm.header.local_count,
            vm.header.remote_count,
            vm.local_list.tickets.len(),
            vm.remote_list.issues.len(),
            vm.detail.local_ticket.as_ref().and_then(|t| t.id.clone()),
            vm.detail.remote_issue.as_ref().map(|i| i.id.clone()),
        )
    );
}

#[test]
fn test_view_model_toggle_view() {
    let state = state_with_data();
    let vm1 = compute_remote_view_model(&state, 20);

    // Initially focused on local
    insta::assert_debug_snapshot!(
        "toggle_view_initial",
        (vm1.local_list.is_focused, vm1.remote_list.is_focused)
    );

    // Toggle to remote
    let state = reduce_remote_state(state, RemoteAction::ToggleView, 20);
    let vm2 = compute_remote_view_model(&state, 20);
    insta::assert_debug_snapshot!(
        "toggle_view_toggled",
        (vm2.local_list.is_focused, vm2.remote_list.is_focused)
    );
}

#[test]
fn test_view_model_with_search() {
    let mut state = state_with_data();
    // Give tickets more specific titles for search
    state.local_tickets[0].title = Some("Fix bug in login".to_string());
    state.local_tickets[1].title = Some("Add new feature".to_string());
    state.local_tickets[2].title = Some("Fix bug in checkout".to_string());

    state.search_query = "bug".to_string();
    let vm = compute_remote_view_model(&state, 20);

    insta::assert_debug_snapshot!(
        "remote_search",
        (
            &vm.search.query,
            vm.search.result_count,
            vm.local_list.tickets.len(),
        )
    );
}

#[test]
fn test_view_model_search_focused() {
    let mut state = state_with_data();
    state.search_focused = true;
    let vm = compute_remote_view_model(&state, 20);

    insta::assert_debug_snapshot!(
        "search_focused",
        (
            vm.search.is_focused,
            vm.local_list.is_focused,
            vm.remote_list.is_focused
        )
    );
}

#[test]
fn test_view_model_detail_visible() {
    let mut state = state_with_data();
    state.show_detail = true;
    let vm = compute_remote_view_model(&state, 20);

    insta::assert_debug_snapshot!(
        "detail_visible",
        (vm.detail.is_visible, vm.detail.view_mode)
    );
}

#[test]
fn test_view_model_loading_state() {
    let mut state = state_with_data();
    state.is_loading = true;
    let vm = compute_remote_view_model(&state, 20);

    assert!(vm.is_loading);
}

#[test]
fn test_view_model_with_selections() {
    let mut state = state_with_data();
    state.local_selected_ids.insert("j-1".to_string());
    state.local_selected_ids.insert("j-2".to_string());

    let vm = compute_remote_view_model(&state, 20);

    insta::assert_debug_snapshot!(
        "with_selections",
        (
            vm.local_list.selected_ids.len(),
            vm.local_list.selected_index
        )
    );
}

#[test]
fn test_view_model_modal_states() {
    // Help modal
    let mut state = state_with_data();
    state.show_help_modal = true;
    let vm = compute_remote_view_model(&state, 20);
    insta::assert_debug_snapshot!("help_modal", (vm.modal.show_help, vm.modal.show_filter));

    // Filter modal
    let mut state = state_with_data();
    state.filter_modal = Some(FilterState::default());
    let vm = compute_remote_view_model(&state, 20);
    insta::assert_debug_snapshot!(
        "filter_modal",
        (
            vm.modal.show_help,
            vm.modal.show_filter,
            vm.modal.filter_state.is_some()
        )
    );

    // Error modal
    let mut state = state_with_data();
    state.show_error_modal = true;
    state.last_error = Some(("Error".to_string(), "Details".to_string()));
    let vm = compute_remote_view_model(&state, 20);
    insta::assert_debug_snapshot!(
        "error_modal",
        (vm.modal.show_error, vm.modal.error.is_some())
    );
}

#[test]
fn test_view_model_link_mode() {
    let mut state = state_with_data();
    state.link_mode = Some(LinkModeState::new(
        ViewMode::Local,
        "j-1".to_string(),
        "Test ticket".to_string(),
    ));
    let vm = compute_remote_view_model(&state, 20);

    insta::assert_debug_snapshot!(
        "link_mode",
        (
            vm.modal.link_mode_active,
            vm.modal.link_mode_state.as_ref().map(|s| &s.source_id),
        )
    );
}

#[test]
fn test_view_model_header_filters() {
    let mut state = state_with_data();
    state.active_filters.assignee = Some("alice".to_string());
    let vm = compute_remote_view_model(&state, 20);

    assert!(vm.header.has_active_filters);
}

#[test]
fn test_view_model_scroll_state() {
    let state = state_with_many_items(30, 30);
    let state = reduce_remote_state(state, RemoteAction::GoToBottom, 10);

    let vm = compute_remote_view_model(&state, 10);

    insta::assert_debug_snapshot!(
        "scroll_state",
        (
            vm.local_list.selected_index,
            vm.local_list.scroll_offset,
            vm.local_list.visible_count,
        )
    );
}

// ============================================================================
// Reducer Action Sequence Tests
// ============================================================================

#[test]
fn test_navigation_sequence_local() {
    let state = state_with_many_items(10, 10);

    // Navigate down 5 times
    let state = reduce_remote_state(state, RemoteAction::MoveDown, 20);
    let state = reduce_remote_state(state, RemoteAction::MoveDown, 20);
    let state = reduce_remote_state(state, RemoteAction::MoveDown, 20);
    let state = reduce_remote_state(state, RemoteAction::MoveDown, 20);
    let state = reduce_remote_state(state, RemoteAction::MoveDown, 20);

    insta::assert_debug_snapshot!(
        "nav_local_down_5",
        (state.local_selected_index, state.local_scroll_offset)
    );
}

#[test]
fn test_navigation_sequence_remote() {
    let state = state_with_many_items(10, 10);
    let state = reduce_remote_state(state, RemoteAction::ToggleView, 20);

    // Navigate down in remote view
    let state = reduce_remote_state(state, RemoteAction::MoveDown, 20);
    let state = reduce_remote_state(state, RemoteAction::MoveDown, 20);
    let state = reduce_remote_state(state, RemoteAction::MoveDown, 20);

    insta::assert_debug_snapshot!(
        "nav_remote_down_3",
        (
            state.active_view,
            state.remote_selected_index,
            state.remote_scroll_offset
        )
    );
}

#[test]
fn test_navigation_with_scroll() {
    let state = state_with_many_items(30, 30);

    // Navigate to bottom in small list view
    let state = reduce_remote_state(state, RemoteAction::GoToBottom, 5);

    insta::assert_debug_snapshot!(
        "nav_scroll_bottom",
        (state.local_selected_index, state.local_scroll_offset)
    );

    // Navigate to top
    let state = reduce_remote_state(state, RemoteAction::GoToTop, 5);
    insta::assert_debug_snapshot!(
        "nav_scroll_top",
        (state.local_selected_index, state.local_scroll_offset)
    );
}

#[test]
fn test_page_navigation() {
    let state = state_with_many_items(50, 50);
    let state = reduce_remote_state(state, RemoteAction::GoToBottom, 20);
    let initial_idx = state.local_selected_index;

    // Page up
    let state = reduce_remote_state(state, RemoteAction::PageUp, 20);
    insta::assert_debug_snapshot!("page_up", (initial_idx, state.local_selected_index));
}

#[test]
fn test_extend_selection_sequence() {
    let state = state_with_data();

    // Extend selection downward
    let state = reduce_remote_state(state, RemoteAction::MoveDownExtendSelection, 20);
    let state = reduce_remote_state(state, RemoteAction::MoveDownExtendSelection, 20);

    insta::assert_debug_snapshot!(
        "extend_selection",
        (state.local_selected_index, state.local_selected_ids.len(),)
    );
}

#[test]
fn test_search_flow() {
    let state = state_with_many_items(10, 10);
    let state = reduce_remote_state(state, RemoteAction::MoveDown, 20);
    let state = reduce_remote_state(state, RemoteAction::MoveDown, 20);
    let initial_idx = state.local_selected_index;

    // Focus search
    let state = reduce_remote_state(state, RemoteAction::FocusSearch, 20);
    assert!(state.search_focused);

    // Update search (resets selection)
    let state = reduce_remote_state(state, RemoteAction::UpdateSearch("test".to_string()), 20);

    insta::assert_debug_snapshot!(
        "search_flow",
        (initial_idx, state.local_selected_index, &state.search_query)
    );

    // Exit search preserving query
    let state = reduce_remote_state(state, RemoteAction::ExitSearch, 20);
    assert!(!state.search_focused);
    assert_eq!(state.search_query, "test");
}

#[test]
fn test_search_clear_and_exit() {
    let mut state = state_with_data();
    state.search_focused = true;
    state.search_query = "test query".to_string();

    let state = reduce_remote_state(state, RemoteAction::ClearSearchAndExit, 20);

    assert!(!state.search_focused);
    assert!(state.search_query.is_empty());
}

#[test]
fn test_selection_flow() {
    let state = state_with_data();

    // Toggle selection
    let state = reduce_remote_state(state, RemoteAction::ToggleSelection, 20);
    assert_eq!(state.local_selected_ids.len(), 1);

    // Move and toggle again
    let state = reduce_remote_state(state, RemoteAction::MoveDown, 20);
    let state = reduce_remote_state(state, RemoteAction::ToggleSelection, 20);
    assert_eq!(state.local_selected_ids.len(), 2);

    // Clear selection
    let state = reduce_remote_state(state, RemoteAction::ClearSelection, 20);
    assert!(state.local_selected_ids.is_empty());
}

#[test]
fn test_select_all() {
    let state = state_with_data();
    let state = reduce_remote_state(state, RemoteAction::SelectAll, 20);

    assert_eq!(state.local_selected_ids.len(), 3);
}

#[test]
fn test_link_mode_flow() {
    let state = state_with_data();

    // Start link mode from local view
    let state = reduce_remote_state(state, RemoteAction::StartLinkMode, 20);
    assert!(state.link_mode.is_some());
    assert_eq!(state.active_view, ViewMode::Remote); // Switched to remote for target selection

    // Cancel link mode
    let state = reduce_remote_state(state, RemoteAction::CancelLinkMode, 20);
    assert!(state.link_mode.is_none());
    assert_eq!(state.active_view, ViewMode::Local); // Restored
}

#[test]
fn test_modal_flow() {
    let state = state_with_data();

    // Show help
    let state = reduce_remote_state(state, RemoteAction::ShowHelp, 20);
    assert!(state.show_help_modal);

    // Hide help
    let state = reduce_remote_state(state, RemoteAction::HideHelp, 20);
    assert!(!state.show_help_modal);

    // Show filter modal
    let state = reduce_remote_state(state, RemoteAction::ShowFilterModal, 20);
    assert!(state.filter_modal.is_some());

    // Hide filter modal
    let state = reduce_remote_state(state, RemoteAction::HideFilterModal, 20);
    assert!(state.filter_modal.is_none());
}

#[test]
fn test_quit() {
    let state = state_with_data();
    let state = reduce_remote_state(state, RemoteAction::Quit, 20);

    assert!(state.should_exit);
}

// ============================================================================
// Key Mapping Tests
// ============================================================================

#[test]
fn test_key_mapping_normal_navigation() {
    let state = default_state();

    assert_eq!(
        key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, &state),
        Some(RemoteAction::MoveDown)
    );
    assert_eq!(
        key_to_action(KeyCode::Down, KeyModifiers::NONE, &state),
        Some(RemoteAction::MoveDown)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('k'), KeyModifiers::NONE, &state),
        Some(RemoteAction::MoveUp)
    );
    assert_eq!(
        key_to_action(KeyCode::Up, KeyModifiers::NONE, &state),
        Some(RemoteAction::MoveUp)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('g'), KeyModifiers::NONE, &state),
        Some(RemoteAction::GoToTop)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('G'), KeyModifiers::NONE, &state),
        Some(RemoteAction::GoToBottom)
    );
    assert_eq!(
        key_to_action(KeyCode::PageUp, KeyModifiers::NONE, &state),
        Some(RemoteAction::PageUp)
    );
    assert_eq!(
        key_to_action(KeyCode::PageDown, KeyModifiers::NONE, &state),
        Some(RemoteAction::PageDown)
    );
}

#[test]
fn test_key_mapping_shift_navigation() {
    let state = default_state();

    assert_eq!(
        key_to_action(KeyCode::Char('J'), KeyModifiers::SHIFT, &state),
        Some(RemoteAction::MoveDownExtendSelection)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('K'), KeyModifiers::SHIFT, &state),
        Some(RemoteAction::MoveUpExtendSelection)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('G'), KeyModifiers::SHIFT, &state),
        Some(RemoteAction::GoToBottom)
    );
}

#[test]
fn test_key_mapping_view_and_selection() {
    let state = default_state();

    assert_eq!(
        key_to_action(KeyCode::Tab, KeyModifiers::NONE, &state),
        Some(RemoteAction::ToggleView)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('d'), KeyModifiers::NONE, &state),
        Some(RemoteAction::ToggleDetail)
    );
    assert_eq!(
        key_to_action(KeyCode::Char(' '), KeyModifiers::NONE, &state),
        Some(RemoteAction::ToggleSelection)
    );
}

#[test]
fn test_key_mapping_operations() {
    let state = default_state();

    assert_eq!(
        key_to_action(KeyCode::Char('r'), KeyModifiers::NONE, &state),
        Some(RemoteAction::Fetch)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('P'), KeyModifiers::SHIFT, &state),
        Some(RemoteAction::Push)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('s'), KeyModifiers::NONE, &state),
        Some(RemoteAction::StartSync)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('l'), KeyModifiers::NONE, &state),
        Some(RemoteAction::StartLinkMode)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('u'), KeyModifiers::NONE, &state),
        Some(RemoteAction::Unlink)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('e'), KeyModifiers::NONE, &state),
        Some(RemoteAction::EditLocal)
    );
    assert_eq!(
        key_to_action(KeyCode::Enter, KeyModifiers::NONE, &state),
        Some(RemoteAction::EditLocal)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('o'), KeyModifiers::NONE, &state),
        Some(RemoteAction::OpenRemote)
    );
}

#[test]
fn test_key_mapping_modals() {
    let state = default_state();

    assert_eq!(
        key_to_action(KeyCode::Char('?'), KeyModifiers::NONE, &state),
        Some(RemoteAction::ShowHelp)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('f'), KeyModifiers::NONE, &state),
        Some(RemoteAction::ShowFilterModal)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('q'), KeyModifiers::NONE, &state),
        Some(RemoteAction::Quit)
    );
}

#[test]
fn test_key_mapping_search_mode() {
    let mut state = default_state();
    state.search_focused = true;

    assert_eq!(
        key_to_action(KeyCode::Esc, KeyModifiers::NONE, &state),
        Some(RemoteAction::ClearSearchAndExit)
    );
    assert_eq!(
        key_to_action(KeyCode::Enter, KeyModifiers::NONE, &state),
        Some(RemoteAction::ExitSearch)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('q'), KeyModifiers::CONTROL, &state),
        Some(RemoteAction::Quit)
    );
    // Regular keys should return None (handled by search input)
    assert_eq!(
        key_to_action(KeyCode::Char('a'), KeyModifiers::NONE, &state),
        None
    );
}

#[test]
fn test_key_mapping_help_modal() {
    let mut state = default_state();
    state.show_help_modal = true;

    assert_eq!(
        key_to_action(KeyCode::Esc, KeyModifiers::NONE, &state),
        Some(RemoteAction::HideHelp)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('?'), KeyModifiers::NONE, &state),
        Some(RemoteAction::HideHelp)
    );
    // Other keys should return None
    assert_eq!(
        key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, &state),
        None
    );
}

#[test]
fn test_key_mapping_link_mode() {
    let mut state = default_state();
    state.link_mode = Some(LinkModeState::new(
        ViewMode::Local,
        "j-1".to_string(),
        "Test".to_string(),
    ));

    assert_eq!(
        key_to_action(KeyCode::Esc, KeyModifiers::NONE, &state),
        Some(RemoteAction::CancelLinkMode)
    );
    assert_eq!(
        key_to_action(KeyCode::Enter, KeyModifiers::NONE, &state),
        Some(RemoteAction::ConfirmLink)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, &state),
        Some(RemoteAction::MoveDown)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('k'), KeyModifiers::NONE, &state),
        Some(RemoteAction::MoveUp)
    );
}

#[test]
fn test_key_mapping_filter_modal() {
    let mut state = default_state();
    state.filter_modal = Some(FilterState::default());

    assert_eq!(
        key_to_action(KeyCode::Esc, KeyModifiers::NONE, &state),
        Some(RemoteAction::HideFilterModal)
    );
    // Other keys should return None (handled by modal component)
    assert_eq!(
        key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, &state),
        None
    );
}

#[test]
fn test_key_mapping_unknown_keys() {
    let state = default_state();

    assert_eq!(
        key_to_action(KeyCode::Char('x'), KeyModifiers::NONE, &state),
        None
    );
    assert_eq!(
        key_to_action(KeyCode::F(1), KeyModifiers::NONE, &state),
        None
    );
    assert_eq!(
        key_to_action(KeyCode::Home, KeyModifiers::NONE, &state),
        None
    );
}

// ============================================================================
// Shortcuts Tests
// ============================================================================

#[test]
fn test_shortcuts_normal_mode() {
    let state = default_state();
    let shortcuts = compute_shortcuts(&state);

    assert!(shortcuts.iter().any(|s| s.key == "Tab"));
    assert!(shortcuts.iter().any(|s| s.key == "q"));
    assert!(shortcuts.iter().any(|s| s.key == "?"));
}

#[test]
fn test_shortcuts_search_mode() {
    let mut state = default_state();
    state.search_focused = true;
    let shortcuts = compute_shortcuts(&state);

    assert!(shortcuts.iter().any(|s| s.key == "Enter"));
    assert!(shortcuts.iter().any(|s| s.key == "Esc"));
}

#[test]
fn test_shortcuts_help_modal() {
    let mut state = default_state();
    state.show_help_modal = true;
    let shortcuts = compute_shortcuts(&state);

    assert!(shortcuts.iter().any(|s| s.key == "Esc"));
    assert!(shortcuts.iter().any(|s| s.key == "?"));
}

#[test]
fn test_shortcuts_link_mode() {
    let mut state = default_state();
    state.link_mode = Some(LinkModeState::new(
        ViewMode::Local,
        "j-1".to_string(),
        "Test".to_string(),
    ));
    let shortcuts = compute_shortcuts(&state);

    assert!(shortcuts.iter().any(|s| s.key == "Enter"));
    assert!(shortcuts.iter().any(|s| s.key == "Esc"));
    assert!(shortcuts.iter().any(|s| s.key == "j/k"));
}

// ============================================================================
// Helper Function Tests
// ============================================================================

#[test]
fn test_get_local_ticket_at() {
    let state = state_with_data();

    let ticket = get_local_ticket_at(&state, 0);
    assert!(ticket.is_some());
    assert_eq!(ticket.unwrap().id, Some("j-1".to_string()));

    let ticket = get_local_ticket_at(&state, 2);
    assert!(ticket.is_some());
    assert_eq!(ticket.unwrap().id, Some("j-3".to_string()));

    assert!(get_local_ticket_at(&state, 10).is_none());
}

#[test]
fn test_get_remote_issue_at() {
    let state = state_with_data();

    let issue = get_remote_issue_at(&state, 0);
    assert!(issue.is_some());
    assert_eq!(issue.unwrap().id, "GH-1");

    let issue = get_remote_issue_at(&state, 2);
    assert!(issue.is_some());
    assert_eq!(issue.unwrap().id, "GH-3");

    assert!(get_remote_issue_at(&state, 10).is_none());
}

#[test]
fn test_get_selected_local_ticket() {
    let mut state = state_with_data();
    state.local_selected_index = 1;

    let ticket = get_selected_local_ticket(&state);
    assert!(ticket.is_some());
    assert_eq!(ticket.unwrap().id, Some("j-2".to_string()));
}

#[test]
fn test_get_selected_remote_issue() {
    let mut state = state_with_data();
    state.remote_selected_index = 1;

    let issue = get_selected_remote_issue(&state);
    assert!(issue.is_some());
    assert_eq!(issue.unwrap().id, "GH-2");
}

#[test]
fn test_adjust_scroll_edge_cases() {
    // Within bounds - no change
    assert_eq!(adjust_scroll(0, 5, 10), 0);
    assert_eq!(adjust_scroll(5, 8, 10), 5);

    // Below visible - scroll down
    assert_eq!(adjust_scroll(0, 15, 10), 6);

    // Above visible - scroll up
    assert_eq!(adjust_scroll(10, 5, 10), 5);

    // Zero height
    assert_eq!(adjust_scroll(5, 10, 0), 0);
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_navigation_empty_state() {
    let state = default_state();

    // Navigation on empty state should not panic
    let state = reduce_remote_state(state, RemoteAction::MoveDown, 20);
    assert_eq!(state.local_selected_index, 0);

    let state = reduce_remote_state(state, RemoteAction::MoveUp, 20);
    assert_eq!(state.local_selected_index, 0);

    let state = reduce_remote_state(state, RemoteAction::GoToBottom, 20);
    assert_eq!(state.local_selected_index, 0);
}

#[test]
fn test_view_with_rich_data() {
    let local_ticket = TicketBuilder::new("j-rich1")
        .title("Important bug fix")
        .status(TicketStatus::InProgress)
        .ticket_type(TicketType::Bug)
        .priority(TicketPriority::P0)
        .dep("j-dep1")
        .parent("j-parent")
        .build();

    let remote_issue = RemoteIssueBuilder::new("GH-rich1")
        .title("Critical issue")
        .body("This is a detailed description")
        .status(RemoteStatus::Open)
        .priority(1)
        .assignee("alice")
        .label("bug")
        .label("urgent")
        .team("Core")
        .project("Backend")
        .build();

    let state = RemoteState {
        local_tickets: vec![local_ticket],
        remote_issues: vec![remote_issue],
        ..default_state()
    };

    let vm = compute_remote_view_model(&state, 20);

    assert_eq!(vm.local_list.tickets.len(), 1);
    assert_eq!(vm.remote_list.issues.len(), 1);
    assert_eq!(
        vm.detail.local_ticket.as_ref().and_then(|t| t.id.clone()),
        Some("j-rich1".to_string())
    );
    assert_eq!(
        vm.detail.remote_issue.as_ref().map(|i| i.id.clone()),
        Some("GH-rich1".to_string())
    );
}

#[test]
fn test_complex_user_session() {
    // Simulate a realistic user session
    let state = state_with_many_items(20, 20);

    // Navigate down, toggle view, navigate, search, toggle selection, etc.
    let state = reduce_remote_state(state, RemoteAction::MoveDown, 10);
    let state = reduce_remote_state(state, RemoteAction::MoveDown, 10);
    let state = reduce_remote_state(state, RemoteAction::MoveDown, 10);
    assert_eq!(state.local_selected_index, 3);

    let state = reduce_remote_state(state, RemoteAction::ToggleView, 10);
    assert_eq!(state.active_view, ViewMode::Remote);

    let state = reduce_remote_state(state, RemoteAction::MoveDown, 10);
    let state = reduce_remote_state(state, RemoteAction::MoveDown, 10);
    assert_eq!(state.remote_selected_index, 2);

    let state = reduce_remote_state(state, RemoteAction::ToggleView, 10);
    assert_eq!(state.active_view, ViewMode::Local);
    assert_eq!(state.local_selected_index, 3); // Preserved

    let state = reduce_remote_state(state, RemoteAction::FocusSearch, 10);
    assert!(state.search_focused);

    let state = reduce_remote_state(state, RemoteAction::UpdateSearch("test".to_string()), 10);
    assert_eq!(state.search_query, "test");
    assert_eq!(state.local_selected_index, 0); // Reset on search

    let state = reduce_remote_state(state, RemoteAction::ExitSearch, 10);
    assert!(!state.search_focused);
    assert_eq!(state.search_query, "test"); // Preserved

    let state = reduce_remote_state(state, RemoteAction::ToggleSelection, 10);
    assert_eq!(state.local_selected_ids.len(), 1);

    let state = reduce_remote_state(state, RemoteAction::ShowHelp, 10);
    assert!(state.show_help_modal);

    let state = reduce_remote_state(state, RemoteAction::HideHelp, 10);
    assert!(!state.show_help_modal);
}

// ============================================================================
// Linked Tickets Fixture Tests
// ============================================================================

#[test]
#[serial]
fn test_linked_tickets_fixture_loads() {
    let _guard = FixtureGuard::new("linked_tickets");

    // Load tickets from the fixture using the async runtime
    let tickets: Vec<janus::types::TicketMetadata> = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async { janus::ticket::get_all_tickets().await })
        .expect("Failed to load tickets from linked_tickets fixture");

    // Verify we have 3 tickets
    assert_eq!(
        tickets.len(),
        3,
        "Expected 3 tickets in linked_tickets fixture"
    );

    // Find each ticket by ID
    let linear_ticket = tickets.iter().find(|t| t.id.as_deref() == Some("j-lin1"));
    let github_ticket = tickets.iter().find(|t| t.id.as_deref() == Some("j-gh01"));
    let unlinked_ticket = tickets.iter().find(|t| t.id.as_deref() == Some("j-unlk"));

    // Verify all tickets exist
    assert!(
        linear_ticket.is_some(),
        "Linear-linked ticket j-lin1 should exist"
    );
    assert!(
        github_ticket.is_some(),
        "GitHub-linked ticket j-gh01 should exist"
    );
    assert!(
        unlinked_ticket.is_some(),
        "Unlinked ticket j-unlk should exist"
    );

    // Verify remote references
    let linear_ticket = linear_ticket.unwrap();
    assert_eq!(
        linear_ticket.remote.as_deref(),
        Some("linear:acme/ENG-123"),
        "Linear ticket should have correct remote reference"
    );

    let github_ticket = github_ticket.unwrap();
    assert_eq!(
        github_ticket.remote.as_deref(),
        Some("github:owner/repo/456"),
        "GitHub ticket should have correct remote reference"
    );

    let unlinked_ticket = unlinked_ticket.unwrap();
    assert!(
        unlinked_ticket.remote.is_none(),
        "Unlinked ticket should have no remote reference"
    );
}

#[test]
#[serial]
fn test_linked_tickets_fixture_statuses() {
    let _guard = FixtureGuard::new("linked_tickets");

    let tickets: Vec<janus::types::TicketMetadata> = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async { janus::ticket::get_all_tickets().await })
        .expect("Failed to load tickets from linked_tickets fixture");

    // Verify statuses
    let linear_ticket = tickets
        .iter()
        .find(|t| t.id.as_deref() == Some("j-lin1"))
        .unwrap();
    assert_eq!(linear_ticket.status, Some(TicketStatus::InProgress));

    let github_ticket = tickets
        .iter()
        .find(|t| t.id.as_deref() == Some("j-gh01"))
        .unwrap();
    assert_eq!(github_ticket.status, Some(TicketStatus::New));

    let unlinked_ticket = tickets
        .iter()
        .find(|t| t.id.as_deref() == Some("j-unlk"))
        .unwrap();
    assert_eq!(unlinked_ticket.status, Some(TicketStatus::Next));
}

#[test]
fn test_mock_linked_ticket_helper() {
    // Test that the mock_linked_ticket helper works correctly
    let ticket = mock_linked_ticket("j-test", "linear:org/PROJ-999", TicketStatus::InProgress);

    assert_eq!(ticket.id, Some("j-test".to_string()));
    assert_eq!(ticket.remote, Some("linear:org/PROJ-999".to_string()));
    assert_eq!(ticket.status, Some(TicketStatus::InProgress));
}

#[test]
fn test_view_model_with_linked_tickets() {
    // Test that linked tickets work correctly in the view model
    let linked_ticket = mock_linked_ticket(
        "j-linked",
        "github:owner/repo/123",
        TicketStatus::InProgress,
    );
    let unlinked_ticket = mock_ticket("j-unlinked", TicketStatus::New);

    let remote_issue = mock_remote_issue("123", RemoteStatus::Open);

    let state = RemoteState {
        local_tickets: vec![linked_ticket, unlinked_ticket],
        remote_issues: vec![remote_issue],
        ..default_state()
    };

    let vm = compute_remote_view_model(&state, 20);

    // Verify view model contains both tickets
    assert_eq!(vm.local_list.tickets.len(), 2);
    assert_eq!(vm.remote_list.issues.len(), 1);

    // Verify first ticket has remote reference
    assert_eq!(
        state.local_tickets[0].remote,
        Some("github:owner/repo/123".to_string())
    );
    // Verify second ticket has no remote reference
    assert!(state.local_tickets[1].remote.is_none());
}
