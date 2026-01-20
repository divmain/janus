//! Shared handler utilities for TUI components
//!
//! Provides common event handling logic that can be reused across different
//! TUI views (board, view, remote) to reduce code duplication.

use iocraft::prelude::{KeyCode, KeyModifiers};

/// Action to take based on search input handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchAction {
    /// Clear search query and exit search mode (Esc)
    ClearAndExit,
    /// Exit search mode, keep query (Enter/Tab)
    Exit,
    /// Exit application (Ctrl+Q)
    Quit,
    /// Let the search box component handle the key
    Continue,
}

/// Handle common search input keyboard events
///
/// This function implements the standard search input behavior used across
/// all TUI views:
/// - Esc: Clear search query and exit search mode
/// - Enter/Tab: Exit search mode (keep query)
/// - Ctrl+Q: Exit application (if enabled)
/// - Other keys: Allow the search box component to handle them
///
/// # Parameters
/// - `key_code`: The key that was pressed
/// - `modifiers`: Keyboard modifiers (Ctrl, Shift, etc.)
/// - `handle_ctrl_q`: Whether to handle Ctrl+Q for application exit
///
/// # Returns
/// A `SearchAction` indicating what action to take
///
/// # Examples
/// ```ignore
/// // In board search handler
/// match handle_search_input(code, modifiers, true) {
///     SearchAction::ClearAndExit => {
///         ctx.search_query.set(String::new());
///         ctx.search_focused.set(false);
///         return HandleResult::Handled;
///     }
///     SearchAction::Exit => {
///         ctx.search_focused.set(false);
///         return HandleResult::Handled;
///     }
///     SearchAction::Quit => {
///         ctx.should_exit.set(true);
///         return HandleResult::Handled;
///     }
///     SearchAction::Continue => {
///         return HandleResult::Handled;
///     }
/// }
/// ```
pub fn handle_search_input(
    key_code: KeyCode,
    modifiers: KeyModifiers,
    handle_ctrl_q: bool,
) -> SearchAction {
    match key_code {
        KeyCode::Esc => SearchAction::ClearAndExit,
        KeyCode::Enter | KeyCode::Tab => SearchAction::Exit,
        KeyCode::Char('q') if handle_ctrl_q && modifiers.contains(KeyModifiers::CONTROL) => {
            SearchAction::Quit
        }
        _ => SearchAction::Continue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_search_input_esc() {
        let action = handle_search_input(KeyCode::Esc, KeyModifiers::NONE, true);
        assert_eq!(action, SearchAction::ClearAndExit);
    }

    #[test]
    fn test_handle_search_input_enter() {
        let action = handle_search_input(KeyCode::Enter, KeyModifiers::NONE, true);
        assert_eq!(action, SearchAction::Exit);
    }

    #[test]
    fn test_handle_search_input_tab() {
        let action = handle_search_input(KeyCode::Tab, KeyModifiers::NONE, true);
        assert_eq!(action, SearchAction::Exit);
    }

    #[test]
    fn test_handle_search_input_ctrl_q_enabled() {
        let action = handle_search_input(KeyCode::Char('q'), KeyModifiers::CONTROL, true);
        assert_eq!(action, SearchAction::Quit);
    }

    #[test]
    fn test_handle_search_input_ctrl_q_disabled() {
        let action = handle_search_input(KeyCode::Char('q'), KeyModifiers::CONTROL, false);
        assert_eq!(action, SearchAction::Continue);
    }

    #[test]
    fn test_handle_search_input_other_key() {
        let action = handle_search_input(KeyCode::Char('a'), KeyModifiers::NONE, true);
        assert_eq!(action, SearchAction::Continue);
    }

    #[test]
    fn test_handle_search_input_regular_q() {
        let action = handle_search_input(KeyCode::Char('q'), KeyModifiers::NONE, true);
        assert_eq!(action, SearchAction::Continue);
    }
}
