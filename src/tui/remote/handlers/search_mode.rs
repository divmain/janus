//! Search mode event handler

use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::tui::handlers::{handle_search_input, SearchAction};

use super::context::HandlerContext;
use super::HandleResult;

/// Handle events when search box is focused
#[allow(dead_code)]
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    match handle_search_input(code, KeyModifiers::NONE) {
        SearchAction::ClearAndExit => {
            ctx.search.set_query(String::new());
            ctx.search.set_focused(false);
            HandleResult::Handled
        }
        SearchAction::Exit => {
            // User pressed Enter - trigger search execution
            ctx.search.orchestrator.trigger_pending();
            ctx.search.set_focused(false);
            HandleResult::Handled
        }
        SearchAction::Quit => {
            // Not handled in remote TUI
            HandleResult::NotHandled
        }
        SearchAction::Continue => HandleResult::Handled,
    }
}
