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

use janus::tui::remote::ViewMode;
use janus::types::{TicketPriority, TicketStatus, TicketType};
use serial_test::serial;



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
// DELETED: All view model snapshot tests removed per section 1.4 of TEST_REVIEW.md

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
// Edge Case Tests
// ============================================================================

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
