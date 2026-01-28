//! Search mode event handler

use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::tui::handlers::{SearchAction, handle_search_input};

use super::HandleResult;
use super::context::BoardHandlerContext;

/// Handle events when search is focused
pub fn handle(
    ctx: &mut BoardHandlerContext<'_>,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> HandleResult {
    match handle_search_input(code, modifiers) {
        SearchAction::ClearAndExit => {
            ctx.search_query.set(String::new());
            ctx.search_focused.set(false);
            HandleResult::Handled
        }
        SearchAction::Exit => {
            // User pressed Enter - trigger search execution
            ctx.search_orchestrator.trigger_pending();
            ctx.search_focused.set(false);
            HandleResult::Handled
        }
        SearchAction::Quit => {
            ctx.should_exit.set(true);
            HandleResult::Handled
        }
        SearchAction::Continue => HandleResult::Handled,
    }
}
