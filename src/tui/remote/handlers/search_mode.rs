//! Search mode event handler

use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::tui::handlers::{SearchAction, handle_search_input};

use super::HandleResult;
use super::context::HandlerContext;

/// Handle events when search box is focused
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    match handle_search_input(code, KeyModifiers::NONE, false) {
        SearchAction::ClearAndExit => {
            ctx.search.query.set(String::new());
            ctx.search.focused.set(false);
            HandleResult::Handled
        }
        SearchAction::Exit => {
            ctx.search.focused.set(false);
            HandleResult::Handled
        }
        SearchAction::Quit => {
            // Not handled in remote TUI
            HandleResult::NotHandled
        }
        SearchAction::Continue => HandleResult::Handled,
    }
}
