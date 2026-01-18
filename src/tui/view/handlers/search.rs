//! Search mode event handler

use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::tui::state::Pane;

use super::HandleResult;
use super::context::ViewHandlerContext;

/// Handle events when search pane is active
pub fn handle(
    ctx: &mut ViewHandlerContext<'_>,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> HandleResult {
    match code {
        KeyCode::Esc => {
            // Clear search and switch to list
            ctx.search_query.set(String::new());
            ctx.active_pane.set(Pane::List);
            HandleResult::Handled
        }
        KeyCode::Enter | KeyCode::Tab => {
            // Switch to list pane after searching
            ctx.active_pane.set(Pane::List);
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
