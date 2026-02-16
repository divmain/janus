//! Keyboard event handlers for the kanban board view
//!
//! This module breaks up the complex event handling logic into separate,
//! focused handlers for each mode or operation type.

mod actions;
mod column;
mod context;
mod navigation;
mod search;
mod types;

pub use column::adjust_column_after_toggle;
pub use context::{BoardAsyncHandlers, BoardHandlerContext, FilteredCache};

use super::model::BoardAction;
use crate::tui::handlers::{HandleResult, SearchAction, handle_search_input};
use iocraft::prelude::{KeyCode, KeyModifiers};

/// Main event dispatcher that routes events to the appropriate handler
pub fn handle_key_event(ctx: &mut BoardHandlerContext<'_>, code: KeyCode, modifiers: KeyModifiers) {
    // 1. Search mode has highest priority - captures all input
    if ctx.search_focused.get() && search::handle(ctx, code, modifiers).is_handled() {
        return;
    }

    // 2. Navigation (h/l/j/k/arrows)
    if navigation::handle(ctx, code).is_handled() {
        return;
    }

    // 3. Column toggles (1-5)
    if column::handle_toggle(ctx, code).is_handled() {
        return;
    }

    // 4. Status movement (s/S)
    if column::handle_status_move(ctx, code).is_handled() {
        return;
    }

    // 5. Actions (e, n, q, /)
    actions::handle(ctx, code, modifiers);
}

/// Convert a key event to a BoardAction (pure function)
///
/// This function maps keyboard events to abstract board actions, enabling
/// unit testing of the key mapping logic without any iocraft dependencies.
///
/// Returns `None` if the key doesn't map to any action.
pub fn key_to_action(
    code: KeyCode,
    modifiers: KeyModifiers,
    search_focused: bool,
) -> Option<BoardAction> {
    if search_focused {
        return search_key_to_action(code, modifiers);
    }

    match code {
        // Navigation (arrow keys only - standardized across all views)
        KeyCode::Left => Some(BoardAction::MoveLeft),
        KeyCode::Right => Some(BoardAction::MoveRight),
        KeyCode::Down => Some(BoardAction::MoveDown),
        KeyCode::Up => Some(BoardAction::MoveUp),
        KeyCode::Char('g') => Some(BoardAction::GoToTop),
        KeyCode::Char('G') => Some(BoardAction::GoToBottom),
        KeyCode::PageDown => Some(BoardAction::PageDown),
        KeyCode::PageUp => Some(BoardAction::PageUp),

        // Column toggles
        KeyCode::Char('1') => Some(BoardAction::ToggleColumn(0)),
        KeyCode::Char('2') => Some(BoardAction::ToggleColumn(1)),
        KeyCode::Char('3') => Some(BoardAction::ToggleColumn(2)),
        KeyCode::Char('4') => Some(BoardAction::ToggleColumn(3)),
        KeyCode::Char('5') => Some(BoardAction::ToggleColumn(4)),

        // Status movement
        KeyCode::Char('s') => Some(BoardAction::MoveTicketStatusRight),
        KeyCode::Char('S') => Some(BoardAction::MoveTicketStatusLeft),

        // Actions
        KeyCode::Char('q') if modifiers.contains(KeyModifiers::CONTROL) => Some(BoardAction::Quit),
        KeyCode::Esc => Some(BoardAction::Quit),
        KeyCode::Char('/') => Some(BoardAction::FocusSearch),
        KeyCode::Char('e') | KeyCode::Enter => Some(BoardAction::EditSelected),
        KeyCode::Char('n') => Some(BoardAction::CreateNew),
        KeyCode::Char('y') => Some(BoardAction::CopyTicketId),
        KeyCode::Char('r') => Some(BoardAction::Reload),

        _ => None,
    }
}

/// Convert a key event in search mode to a BoardAction
fn search_key_to_action(code: KeyCode, modifiers: KeyModifiers) -> Option<BoardAction> {
    match handle_search_input(code, modifiers) {
        SearchAction::ClearAndExit => Some(BoardAction::ClearSearchAndExit),
        SearchAction::Exit => Some(BoardAction::ExitSearch),
        SearchAction::Quit => Some(BoardAction::Quit),
        SearchAction::Continue => {
            // For character input, we would need to handle the actual character
            // but that's handled by the search box component's own state
            // Return None to let the search box handle it
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_to_action_navigation() {
        // Arrow keys only
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
    fn test_key_to_action_status_movement() {
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
    fn test_key_to_action_app_actions() {
        // Ctrl+Q and Esc both quit
        assert_eq!(
            key_to_action(KeyCode::Char('q'), KeyModifiers::CONTROL, false),
            Some(BoardAction::Quit)
        );
        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, false),
            Some(BoardAction::Quit)
        );
        // Plain 'q' should not quit
        assert_eq!(
            key_to_action(KeyCode::Char('q'), KeyModifiers::NONE, false),
            None
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
        assert_eq!(
            key_to_action(KeyCode::Char('y'), KeyModifiers::NONE, false),
            Some(BoardAction::CopyTicketId)
        );
    }

    #[test]
    fn test_key_to_action_unknown_key() {
        assert_eq!(
            key_to_action(KeyCode::Char('x'), KeyModifiers::NONE, false),
            None
        );
        assert_eq!(
            key_to_action(KeyCode::F(1), KeyModifiers::NONE, false),
            None
        );
    }

    #[test]
    fn test_key_to_action_search_mode_escape() {
        assert_eq!(
            key_to_action(KeyCode::Esc, KeyModifiers::NONE, true),
            Some(BoardAction::ClearSearchAndExit)
        );
    }

    #[test]
    fn test_key_to_action_search_mode_enter() {
        assert_eq!(
            key_to_action(KeyCode::Enter, KeyModifiers::NONE, true),
            Some(BoardAction::ExitSearch)
        );
    }

    #[test]
    fn test_key_to_action_search_mode_tab() {
        assert_eq!(
            key_to_action(KeyCode::Tab, KeyModifiers::NONE, true),
            Some(BoardAction::ExitSearch)
        );
    }

    #[test]
    fn test_key_to_action_search_mode_ctrl_q() {
        assert_eq!(
            key_to_action(KeyCode::Char('q'), KeyModifiers::CONTROL, true),
            Some(BoardAction::Quit)
        );
    }

    #[test]
    fn test_key_to_action_search_mode_regular_key() {
        // Regular keys in search mode return None (handled by search box)
        assert_eq!(
            key_to_action(KeyCode::Char('a'), KeyModifiers::NONE, true),
            None
        );
    }

    #[test]
    fn test_key_to_action_navigation_ignored_in_search_mode() {
        // Arrow keys in search mode should be handled by search, not navigation
        assert_eq!(key_to_action(KeyCode::Left, KeyModifiers::NONE, true), None);
        assert_eq!(key_to_action(KeyCode::Down, KeyModifiers::NONE, true), None);
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
}
