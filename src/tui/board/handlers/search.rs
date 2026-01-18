//! Search mode event handler

use iocraft::prelude::{KeyCode, KeyModifiers};

use super::HandleResult;
use super::context::BoardHandlerContext;

/// Handle events when search is focused
pub fn handle(
    ctx: &mut BoardHandlerContext<'_>,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> HandleResult {
    match code {
        KeyCode::Esc => {
            // Clear search and exit search mode
            ctx.search_query.set(String::new());
            ctx.search_focused.set(false);
            HandleResult::Handled
        }
        KeyCode::Enter | KeyCode::Tab => {
            // Exit search mode (keep the query)
            ctx.search_focused.set(false);
            HandleResult::Handled
        }
        KeyCode::Char('q') if modifiers.contains(KeyModifiers::CONTROL) => {
            ctx.should_exit.set(true);
            HandleResult::Handled
        }
        _ => {
            // Let the search box component handle other keys
            HandleResult::Handled
        }
    }
}
