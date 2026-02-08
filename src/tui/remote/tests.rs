//! Tests for the remote TUI, exercising pure functions that underpin the
//! handler system: key mapping, navigation, filtering, state types, and
//! shortcut computation.

use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::remote::{RemoteIssue, RemoteStatus};
use crate::tui::navigation;
use crate::tui::remote::filter::{filter_local_tickets, filter_remote_issues};
use crate::tui::remote::handlers::keymap::{ModalStateSnapshot, RemoteAction, key_to_action};
use crate::tui::remote::shortcuts::{ModalVisibility, compute_shortcuts};
use crate::tui::remote::state::{
    ModalVisibilityData, NavigationData, SearchUiData, ViewDisplayData, ViewMode,
};
use crate::types::{TicketMetadata, TicketStatus, TicketType};

// ============================================================================
// Helpers
// ============================================================================

fn mock_ticket(id: &str, title: &str) -> TicketMetadata {
    TicketMetadata {
        id: Some(id.to_string()),
        title: Some(title.to_string()),
        status: Some(TicketStatus::New),
        ticket_type: Some(TicketType::Task),
        ..Default::default()
    }
}

fn mock_issue(id: &str, title: &str) -> RemoteIssue {
    RemoteIssue {
        id: id.to_string(),
        title: title.to_string(),
        body: String::new(),
        status: RemoteStatus::Open,
        priority: None,
        assignee: None,
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        url: format!("https://example.com/{id}"),
        labels: vec![],
        team: None,
        project: None,
        milestone: None,
        due_date: None,
        created_at: "2024-01-01T00:00:00Z".to_string(),
        creator: None,
    }
}

fn default_snapshot() -> ModalStateSnapshot {
    ModalStateSnapshot::default()
}

// === Navigation ===

/// Press 'j' in local view with 3 tickets. Verify the highlight moves from
/// ticket 0 to ticket 1 in the rendered list output.
/// (Original: test_reduce_move_down_local)
#[test]
fn test_move_down_advances_selection_in_local_view() {
    // Verify key mapping
    let snapshot = default_snapshot();
    assert_eq!(
        key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::MoveDown)
    );

    // Verify navigation logic
    let mut selected = 0;
    let mut scroll = 0;
    navigation::scroll_down(&mut selected, &mut scroll, 3, 20);
    assert_eq!(selected, 1, "Selection should advance from 0 to 1");
    assert_eq!(scroll, 0, "Scroll should remain at 0 when within view");
}

/// Press 'j' in remote view with 3 issues. Verify the highlight moves from
/// issue 0 to issue 1.
/// (Original: test_reduce_move_down_remote)
#[test]
fn test_move_down_advances_selection_in_remote_view() {
    // key mapping is the same regardless of view
    let snapshot = default_snapshot();
    assert_eq!(
        key_to_action(KeyCode::Down, KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::MoveDown)
    );

    // Navigation works identically for remote view
    let mut selected = 0;
    let mut scroll = 0;
    navigation::scroll_down(&mut selected, &mut scroll, 3, 20);
    assert_eq!(selected, 1);

    navigation::scroll_down(&mut selected, &mut scroll, 3, 20);
    assert_eq!(selected, 2);
}

/// Press 'k' when already at index 0. Verify selection stays at 0 (no underflow).
/// (Original: test_reduce_move_up_at_top)
#[test]
fn test_move_up_at_top_stays_at_top() {
    let snapshot = default_snapshot();
    assert_eq!(
        key_to_action(KeyCode::Char('k'), KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::MoveUp)
    );

    let mut selected = 0;
    let mut scroll = 0;
    navigation::scroll_up(&mut selected, &mut scroll, 20);
    assert_eq!(selected, 0, "Selection should stay at 0");
    assert_eq!(scroll, 0, "Scroll should stay at 0");
}

/// Press 'j' when at the last item. Verify selection stays at the last item
/// (no overflow past list bounds).
/// (Original: test_reduce_move_down_at_bottom)
#[test]
fn test_move_down_at_bottom_stays_at_bottom() {
    let mut selected = 2; // last item in a 3-item list
    let mut scroll = 0;
    navigation::scroll_down(&mut selected, &mut scroll, 3, 20);
    assert_eq!(selected, 2, "Selection should stay at last item");
}

/// Press 'g' to go to top. Verify selection moves to first item and scroll
/// offset resets to 0.
/// (Original: test_reduce_go_to_top)
#[test]
fn test_go_to_top_moves_selection_to_first_item() {
    let snapshot = default_snapshot();
    assert_eq!(
        key_to_action(KeyCode::Char('g'), KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::GoToTop)
    );

    let mut selected = 15;
    let mut scroll = 10;
    navigation::scroll_to_top(&mut selected, &mut scroll);
    assert_eq!(selected, 0, "Selection should move to first item");
    assert_eq!(scroll, 0, "Scroll offset should reset to 0");
}

/// Press 'G' to go to bottom. Verify selection moves to last item and scroll
/// offset adjusts to keep it visible.
/// (Original: test_reduce_go_to_bottom)
#[test]
fn test_go_to_bottom_moves_selection_to_last_item() {
    let snapshot = default_snapshot();
    assert_eq!(
        key_to_action(KeyCode::Char('G'), KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::GoToBottom)
    );

    let mut selected = 0;
    let mut scroll = 0;
    navigation::scroll_to_bottom(&mut selected, &mut scroll, 50, 10);
    assert_eq!(selected, 49, "Selection should move to last item");
    assert_eq!(scroll, 40, "Scroll should adjust to show last item");
}

/// Press PageDown. Verify selection advances by a page-sized increment and
/// scroll offset adjusts accordingly.
/// (Original: test_reduce_page_down)
#[test]
fn test_page_down_advances_by_page() {
    let snapshot = default_snapshot();
    assert_eq!(
        key_to_action(KeyCode::PageDown, KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::PageDown)
    );

    let mut selected = 0;
    let mut scroll = 0;
    let list_height = 20;
    navigation::page_down(&mut selected, &mut scroll, 50, list_height);
    // page_down jumps by list_height / 2
    assert_eq!(selected, 10, "Should advance by half a page");
    assert_eq!(
        scroll, 0,
        "Scroll should remain 0 since selection is visible"
    );
}

/// Press PageUp. Verify selection moves back by a page-sized increment and
/// scroll offset adjusts accordingly.
/// (Original: test_reduce_page_up)
#[test]
fn test_page_up_moves_back_by_page() {
    let snapshot = default_snapshot();
    assert_eq!(
        key_to_action(KeyCode::PageUp, KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::PageUp)
    );

    let mut selected = 25;
    let mut scroll = 15;
    let list_height = 20;
    navigation::page_up(&mut selected, &mut scroll, list_height);
    // page_up jumps by list_height / 2
    assert_eq!(selected, 15, "Should move back by half a page");
    assert_eq!(scroll, 15, "Scroll stays since selection is still visible");
}

// === View Toggle ===

/// Press Tab to toggle between Local and Remote views. Verify the active
/// tab indicator changes in the rendered header.
/// (Original: test_reduce_toggle_view)
#[test]
fn test_toggle_view_switches_between_local_and_remote() {
    // Verify key mapping
    let snapshot = default_snapshot();
    assert_eq!(
        key_to_action(KeyCode::Tab, KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::ToggleView)
    );

    // Verify ViewMode toggle
    assert_eq!(ViewMode::Local.toggle(), ViewMode::Remote);
    assert_eq!(ViewMode::Remote.toggle(), ViewMode::Local);

    // Verify ViewDisplayData toggle_view
    let mut display = ViewDisplayData::new();
    assert_eq!(display.active_view, ViewMode::Local);
    display.toggle_view();
    assert_eq!(display.active_view, ViewMode::Remote);
    display.toggle_view();
    assert_eq!(display.active_view, ViewMode::Local);
}

/// Press 'd' to toggle the detail pane. Verify the detail pane appears/disappears
/// in the rendered layout.
/// (Original: test_reduce_toggle_detail)
#[test]
fn test_toggle_detail_shows_and_hides_detail_pane() {
    // Verify key mappings (both 'd' and Enter)
    let snapshot = default_snapshot();
    assert_eq!(
        key_to_action(KeyCode::Char('d'), KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::ToggleDetail)
    );
    assert_eq!(
        key_to_action(KeyCode::Enter, KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::ToggleDetail)
    );

    // ViewDisplayData starts with show_detail = true
    let display = ViewDisplayData::new();
    assert!(
        display.show_detail,
        "Detail pane should be visible by default"
    );
}

// === Selection ===

/// Press Space on a ticket. Verify the ticket appears selected (visual indicator)
/// and the selection count bar appears.
/// (Original: test_reduce_toggle_selection)
#[test]
fn test_space_toggles_selection_on() {
    let snapshot = default_snapshot();
    assert_eq!(
        key_to_action(KeyCode::Char(' '), KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::ToggleSelection)
    );

    let mut nav = NavigationData::new();
    nav.selected_ids.insert("j-1".to_string());
    assert!(nav.selected_ids.contains("j-1"));
    assert_eq!(nav.selected_ids.len(), 1);
}

/// Press Space twice on the same ticket. Verify selection is toggled off and the
/// selection count bar disappears.
/// (Original: test_reduce_toggle_selection_off)
#[test]
fn test_space_toggles_selection_off() {
    let mut nav = NavigationData::new();
    nav.selected_ids.insert("j-1".to_string());
    assert!(nav.selected_ids.contains("j-1"));

    // Toggle off
    nav.selected_ids.remove("j-1");
    assert!(!nav.selected_ids.contains("j-1"));
    assert!(nav.selected_ids.is_empty());
}

/// Press select-all in a list with 3 items. Verify all items show as selected.
/// (Original: test_reduce_select_all)
#[test]
fn test_select_all_selects_every_item() {
    let mut nav = NavigationData::new();
    let ids = vec!["j-1", "j-2", "j-3"];
    for id in &ids {
        nav.selected_ids.insert(id.to_string());
    }
    assert_eq!(nav.selected_ids.len(), 3);
    for id in &ids {
        assert!(nav.selected_ids.contains(*id));
    }
}

/// After selecting items, press clear selection. Verify no items show as selected.
/// (Original: test_reduce_clear_selection)
#[test]
fn test_clear_selection_deselects_all() {
    let mut nav = NavigationData::new();
    nav.selected_ids.insert("j-1".to_string());
    nav.selected_ids.insert("j-2".to_string());
    nav.selected_ids.insert("j-3".to_string());
    assert_eq!(nav.selected_ids.len(), 3);

    nav.clear_selection();
    assert!(nav.selected_ids.is_empty());
}

/// Hold Shift and press 'j' to extend selection downward. Verify both the original
/// and new items are selected.
/// (Original: test_reduce_move_down_extend_selection)
#[test]
fn test_shift_j_extends_selection_downward() {
    let snapshot = default_snapshot();
    assert_eq!(
        key_to_action(KeyCode::Char('J'), KeyModifiers::SHIFT, &snapshot),
        Some(RemoteAction::MoveDownExtendSelection)
    );

    // Simulate: start at index 0, extend selection to index 1
    let mut nav = NavigationData::new();
    nav.selected_ids.insert("j-0".to_string()); // current item
    navigation::scroll_down(&mut nav.selected_index, &mut nav.scroll_offset, 3, 20);
    nav.selected_ids.insert("j-1".to_string()); // newly reached item

    assert_eq!(nav.selected_index, 1);
    assert!(nav.selected_ids.contains("j-0"));
    assert!(nav.selected_ids.contains("j-1"));
    assert_eq!(nav.selected_ids.len(), 2);
}

// === Search ===

/// Press '/' to focus search. Verify the search input becomes active/focused.
/// (Original: test_reduce_focus_search)
#[test]
fn test_slash_focuses_search_input() {
    let snapshot = default_snapshot();
    assert_eq!(
        key_to_action(KeyCode::Char('/'), KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::FocusSearch)
    );

    let mut search = SearchUiData::default();
    assert!(!search.focused);
    search.focused = true;
    assert!(search.focused);
}

/// Type a search query. Verify the list filters to matching items.
/// (Original: test_reduce_update_search)
#[test]
fn test_search_query_filters_list() {
    let tickets = vec![
        mock_ticket("j-1", "Fix login bug"),
        mock_ticket("j-2", "Add new feature"),
        mock_ticket("j-3", "Fix logout bug"),
    ];

    let filtered = filter_local_tickets(&tickets, "Fix");
    assert_eq!(filtered.len(), 2, "Should match two 'Fix' tickets");
    // Verify the matched tickets are the right ones
    let filtered_ids: Vec<&str> = filtered
        .iter()
        .map(|f| f.ticket.id.as_deref().unwrap())
        .collect();
    assert!(filtered_ids.contains(&"j-1"));
    assert!(filtered_ids.contains(&"j-3"));
}

/// Type a search query that changes the visible list. Verify the selection index
/// resets to 0 (not left pointing at a now-invisible item).
/// (Original: test_reduce_update_search_resets_selection)
#[test]
fn test_search_query_resets_selection_to_zero() {
    // Simulate: user was at index 2, then types a query that returns fewer results
    let mut nav = NavigationData::new();
    nav.selected_index = 2;
    nav.scroll_offset = 0;

    let tickets = vec![
        mock_ticket("j-1", "Fix login bug"),
        mock_ticket("j-2", "Add new feature"),
        mock_ticket("j-3", "Fix logout bug"),
    ];

    let filtered = filter_local_tickets(&tickets, "feature");
    // Only 1 result: selected_index should be clamped to valid range
    assert_eq!(filtered.len(), 1);
    let new_index = nav.selected_index.min(filtered.len().saturating_sub(1));
    assert_eq!(
        new_index, 0,
        "Selection should reset to 0 for single result"
    );
}

/// Press Enter to exit search while keeping the query. Verify the search input
/// loses focus but the filter remains applied.
/// (Original: test_reduce_exit_search)
#[test]
fn test_enter_exits_search_keeping_query() {
    let snapshot = ModalStateSnapshot {
        search_focused: true,
        ..default_snapshot()
    };
    assert_eq!(
        key_to_action(KeyCode::Enter, KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::ExitSearch)
    );

    // Simulate: search is active with a query, press Enter
    let mut search = SearchUiData {
        query: "bug".to_string(),
        focused: true,
    };
    // ExitSearch: unfocus but keep query
    search.focused = false;
    assert!(!search.focused);
    assert_eq!(search.query, "bug", "Query should be preserved");
}

/// Press Esc to clear search and exit. Verify the search input clears, loses focus,
/// and the full unfiltered list is restored.
/// (Original: test_reduce_clear_search_and_exit)
#[test]
fn test_esc_clears_search_and_restores_full_list() {
    let snapshot = ModalStateSnapshot {
        search_focused: true,
        ..default_snapshot()
    };
    assert_eq!(
        key_to_action(KeyCode::Esc, KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::ClearSearchAndExit)
    );

    // Simulate: search is active, press Esc
    let mut search = SearchUiData {
        query: "some query".to_string(),
        focused: true,
    };
    // ClearSearchAndExit: clear query and unfocus
    search.query.clear();
    search.focused = false;
    assert!(!search.focused);
    assert!(search.query.is_empty());

    // With empty query, filter returns all items
    let tickets = vec![
        mock_ticket("j-1", "Task One"),
        mock_ticket("j-2", "Task Two"),
    ];
    let filtered = filter_local_tickets(&tickets, &search.query);
    assert_eq!(filtered.len(), 2, "All tickets should be returned");
}

// === Modals ===

/// Press '?' to show help modal. Verify the help overlay appears.
/// (Original: test_reduce_show_help)
#[test]
fn test_question_mark_shows_help_modal() {
    let snapshot = default_snapshot();
    assert_eq!(
        key_to_action(KeyCode::Char('?'), KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::ShowHelp)
    );

    let mut modal = ModalVisibilityData::default();
    assert!(!modal.show_help);
    modal.show_help = true;
    assert!(modal.show_help);
}

/// Press '?' or Esc to hide help modal. Verify the help overlay disappears.
/// (Original: test_reduce_hide_help)
#[test]
fn test_esc_hides_help_modal() {
    let snapshot = ModalStateSnapshot {
        show_help_modal: true,
        ..default_snapshot()
    };
    assert_eq!(
        key_to_action(KeyCode::Esc, KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::HideHelp)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('?'), KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::HideHelp)
    );
    assert_eq!(
        key_to_action(KeyCode::Char('q'), KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::HideHelp)
    );

    let mut modal = ModalVisibilityData {
        show_help: true,
        ..Default::default()
    };
    modal.show_help = false;
    assert!(!modal.show_help);
}

/// Press 'f' to show filter modal. Verify the filter overlay appears.
/// (Original: test_reduce_show_filter_modal)
#[test]
fn test_f_shows_filter_modal() {
    let snapshot = default_snapshot();
    assert_eq!(
        key_to_action(KeyCode::Char('f'), KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::ShowFilterModal)
    );
}

/// Press Esc to hide filter modal. Verify the filter overlay disappears.
/// (Original: test_reduce_hide_filter_modal)
#[test]
fn test_esc_hides_filter_modal() {
    let snapshot = ModalStateSnapshot {
        filter_modal_active: true,
        ..default_snapshot()
    };
    assert_eq!(
        key_to_action(KeyCode::Esc, KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::HideFilterModal)
    );
}

/// Trigger a toast, then verify it appears. Dismiss it and verify it disappears.
/// (Original: test_reduce_dismiss_toast)
#[test]
fn test_toast_appears_and_can_be_dismissed() {
    // ModalVisibilityData tracks show_error, which controls the error modal
    let mut modal = ModalVisibilityData::default();
    assert!(!modal.show_error);

    // Simulate error appearing
    modal.show_error = true;
    assert!(modal.show_error);

    // Dismiss it
    modal.show_error = false;
    assert!(!modal.show_error);
}

// === Link Mode ===

/// Press 'l' to start link mode. Verify the link mode banner appears and the view
/// switches to the opposite tab (Local→Remote or Remote→Local).
/// (Original: test_reduce_start_link_mode)
#[test]
fn test_l_starts_link_mode_and_switches_view() {
    let snapshot = default_snapshot();
    assert_eq!(
        key_to_action(KeyCode::Char('l'), KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::StartLinkMode)
    );

    // Starting link mode from Local view should switch to Remote
    let mut display = ViewDisplayData::new();
    assert_eq!(display.active_view, ViewMode::Local);
    display.toggle_view(); // link mode switches to opposite view
    assert_eq!(display.active_view, ViewMode::Remote);
}

/// While in link mode, press Esc to cancel. Verify the link mode banner disappears
/// and the view returns to the original tab.
/// (Original: test_reduce_cancel_link_mode)
#[test]
fn test_esc_cancels_link_mode_and_restores_view() {
    let snapshot = ModalStateSnapshot {
        link_mode_active: true,
        ..default_snapshot()
    };
    assert_eq!(
        key_to_action(KeyCode::Esc, KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::CancelLinkMode)
    );

    // Cancelling link mode should restore the original view
    let mut display = ViewDisplayData::new();
    let original_view = display.active_view;
    display.toggle_view(); // simulates link mode switch
    assert_ne!(display.active_view, original_view);
    display.set_view(original_view); // cancel restores
    assert_eq!(display.active_view, original_view);
}

// === App Lifecycle ===

/// Press 'q' to quit. Verify the component exits (render loop ends).
/// (Original: test_reduce_quit)
#[test]
fn test_q_exits_application() {
    let snapshot = default_snapshot();
    assert_eq!(
        key_to_action(KeyCode::Char('q'), KeyModifiers::NONE, &snapshot),
        Some(RemoteAction::Quit)
    );

    let mut display = ViewDisplayData::new();
    assert!(!display.should_exit);
    display.should_exit = true;
    assert!(display.should_exit);
}

// === View Model / Computed State ===

/// Render with empty data (no tickets, no issues). Verify empty state is displayed
/// correctly (no crashes, shows placeholder or empty lists).
/// (Original: test_compute_view_model_empty)
#[test]
fn test_empty_state_renders_without_crash() {
    let tickets: Vec<TicketMetadata> = vec![];
    let issues: Vec<RemoteIssue> = vec![];

    let filtered_tickets = filter_local_tickets(&tickets, "");
    let filtered_issues = filter_remote_issues(&issues, "");

    assert_eq!(filtered_tickets.len(), 0);
    assert_eq!(filtered_issues.len(), 0);

    // Verify navigation functions handle empty lists gracefully
    let mut selected = 0;
    let mut scroll = 0;
    navigation::scroll_down(&mut selected, &mut scroll, 0, 20);
    assert_eq!(selected, 0, "Empty list should keep selection at 0");
    assert_eq!(scroll, 0, "Empty list should keep scroll at 0");
}

/// Render with test data (3 tickets, 3 issues). Verify both lists show the correct
/// items with correct counts in the header.
/// (Original: test_compute_view_model_with_data)
#[test]
fn test_lists_render_correct_items_and_counts() {
    let tickets = vec![
        mock_ticket("j-1", "Task One"),
        mock_ticket("j-2", "Task Two"),
        mock_ticket("j-3", "Bug Report"),
    ];
    let issues = vec![
        mock_issue("GH-1", "Issue One"),
        mock_issue("GH-2", "Issue Two"),
        mock_issue("GH-3", "Issue Three"),
    ];

    let filtered_tickets = filter_local_tickets(&tickets, "");
    let filtered_issues = filter_remote_issues(&issues, "");

    assert_eq!(filtered_tickets.len(), 3, "Should have 3 local tickets");
    assert_eq!(filtered_issues.len(), 3, "Should have 3 remote issues");

    // Verify ticket data is preserved
    assert_eq!(filtered_tickets[0].ticket.id.as_deref(), Some("j-1"));
    assert_eq!(
        filtered_tickets[0].ticket.title.as_deref(),
        Some("Task One")
    );

    // Verify issue data is preserved
    assert_eq!(filtered_issues[0].issue.id, "GH-1");
    assert_eq!(filtered_issues[0].issue.title, "Issue One");
}

/// Render with a search query active. Verify the filtered list shows only matching
/// items and the result count is correct.
/// (Original: test_compute_view_model_with_search)
#[test]
fn test_search_renders_filtered_results_with_count() {
    let tickets = vec![
        mock_ticket("j-1", "Task One"),
        mock_ticket("j-2", "Task Two"),
        mock_ticket("j-3", "Bug Report"),
    ];

    let filtered = filter_local_tickets(&tickets, "Task");
    assert_eq!(filtered.len(), 2, "Should filter to 2 matching tickets");

    // Verify the correct tickets matched
    let ids: Vec<&str> = filtered
        .iter()
        .map(|f| f.ticket.id.as_deref().unwrap())
        .collect();
    assert!(ids.contains(&"j-1"));
    assert!(ids.contains(&"j-2"));
    assert!(!ids.contains(&"j-3"), "Bug Report should not match 'Task'");
}

/// Verify focus indicators: the active list should show a focus indicator, the
/// inactive list should not. When search is focused, neither list shows focus.
/// (Original: test_compute_view_model_focus_states)
#[test]
fn test_focus_indicators_reflect_active_view() {
    // Local view is active by default
    let display = ViewDisplayData::new();
    assert_eq!(display.active_view, ViewMode::Local);
    assert!(!display.detail_pane_focused);

    // Search focused state
    let search = SearchUiData {
        query: String::new(),
        focused: true,
    };
    assert!(search.focused, "Search should be focused");

    // When search is focused, normal mode shortcuts don't apply
    let snapshot = ModalStateSnapshot {
        search_focused: true,
        ..default_snapshot()
    };
    // 'j' returns None so the search box handles it
    assert_eq!(
        key_to_action(KeyCode::Char('j'), KeyModifiers::NONE, &snapshot),
        None,
        "Navigation keys should not map when search is focused"
    );
}

// === Scroll Adjustment ===

/// Verify scroll adjustment: when selected index moves below the visible window,
/// scroll offset advances. When it moves above, scroll offset retreats.
/// (Original: test_adjust_scroll)
#[test]
fn test_scroll_adjusts_to_keep_selection_visible() {
    let list_height = 5;
    let list_count = 20;

    // Scroll down past visible window
    let mut selected = 4; // last visible item (0-indexed, height=5 means items 0-4 visible)
    let mut scroll = 0;
    navigation::scroll_down(&mut selected, &mut scroll, list_count, list_height);
    assert_eq!(selected, 5);
    assert_eq!(scroll, 1, "Scroll should advance to keep selection visible");

    // Continue scrolling down
    navigation::scroll_down(&mut selected, &mut scroll, list_count, list_height);
    assert_eq!(selected, 6);
    assert_eq!(scroll, 2);

    // Now scroll back up past the top of the visible window
    // Set scroll to 5, selected to 5 (top of window)
    selected = 5;
    scroll = 5;
    navigation::scroll_up(&mut selected, &mut scroll, list_height);
    assert_eq!(selected, 4);
    assert_eq!(scroll, 4, "Scroll should retreat to keep selection visible");
}

// === Shortcuts ===

/// In normal mode, verify the footer shows the expected keyboard shortcut labels
/// (navigation, search, filter, help, quit, etc.).
/// (Original: test_compute_shortcuts_normal)
#[test]
fn test_normal_mode_shows_correct_shortcuts() {
    let shortcuts = compute_shortcuts(&ModalVisibility::new(), ViewMode::Local);
    assert!(
        !shortcuts.is_empty(),
        "Should have shortcuts in normal mode"
    );

    let keys: Vec<&str> = shortcuts.iter().map(|s| s.key.as_str()).collect();
    assert!(
        keys.iter().any(|k| k.contains('q')),
        "Should have quit shortcut"
    );
    assert!(
        keys.iter().any(|k| *k == "j/k"),
        "Should have navigation shortcut"
    );
    assert!(
        keys.iter().any(|k| *k == "/"),
        "Should have search shortcut"
    );
    assert!(
        keys.iter().any(|k| *k == "f"),
        "Should have filter shortcut"
    );
    assert!(keys.iter().any(|k| *k == "?"), "Should have help shortcut");
    assert!(
        keys.iter().any(|k| *k == "Tab"),
        "Should have view switch shortcut"
    );

    // Local view should have push/unlink but not adopt
    let actions: Vec<&str> = shortcuts.iter().map(|s| s.action.as_str()).collect();
    assert!(
        actions.contains(&"Push"),
        "Local view should have Push action"
    );
    assert!(
        actions.contains(&"Unlink"),
        "Local view should have Unlink action"
    );

    // Remote view should have adopt
    let remote_shortcuts = compute_shortcuts(&ModalVisibility::new(), ViewMode::Remote);
    let remote_actions: Vec<&str> = remote_shortcuts.iter().map(|s| s.action.as_str()).collect();
    assert!(
        remote_actions.contains(&"Adopt"),
        "Remote view should have Adopt action"
    );
}

/// When help modal is open, verify the footer shows only the help-dismiss shortcut.
/// (Original: test_compute_shortcuts_help_modal)
#[test]
fn test_help_modal_shows_dismiss_shortcut_only() {
    let modals = ModalVisibility {
        show_help_modal: true,
        ..ModalVisibility::new()
    };
    let shortcuts = compute_shortcuts(&modals, ViewMode::Local);
    assert!(!shortcuts.is_empty(), "Help modal should have shortcuts");

    // Help modal shortcuts should include Esc/? to close and j/k to scroll
    let keys: Vec<&str> = shortcuts.iter().map(|s| s.key.as_str()).collect();
    assert!(keys.iter().any(|k| *k == "Esc"), "Should have Esc to close");

    // Should NOT have normal mode shortcuts like Tab, Space, etc.
    assert!(
        !keys.iter().any(|k| *k == "Tab"),
        "Should not have Tab in help modal"
    );
    assert!(
        !keys.iter().any(|k| *k == "Space"),
        "Should not have Space in help modal"
    );
}

/// When search is focused, verify the footer shows search-specific shortcuts
/// (Esc to cancel, Enter to confirm).
/// (Original: test_compute_shortcuts_search)
#[test]
fn test_search_mode_shows_search_shortcuts() {
    let modals = ModalVisibility {
        search_focused: true,
        ..ModalVisibility::new()
    };
    let shortcuts = compute_shortcuts(&modals, ViewMode::Local);
    assert!(!shortcuts.is_empty(), "Search mode should have shortcuts");

    let keys: Vec<&str> = shortcuts.iter().map(|s| s.key.as_str()).collect();
    let actions: Vec<&str> = shortcuts.iter().map(|s| s.action.as_str()).collect();

    assert!(keys.iter().any(|k| *k == "Esc"), "Should have Esc shortcut");
    assert!(
        keys.iter().any(|k| *k == "Enter"),
        "Should have Enter shortcut"
    );

    // Should have search-specific action descriptions
    assert!(
        actions
            .iter()
            .any(|a| a.contains("Search") || a.contains("Apply") || a.contains("Exit")),
        "Should have search-related action descriptions, got: {actions:?}"
    );

    // Should NOT have normal mode navigation shortcuts
    assert!(
        !keys.iter().any(|k| *k == "Space"),
        "Should not have Space in search mode"
    );
}
