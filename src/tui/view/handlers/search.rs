//! Search mode event handler

use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::tui::handlers::{SearchAction, handle_search_input};
use crate::tui::state::Pane;

use super::HandleResult;
use super::context::ViewHandlerContext;

/// Handle events when search pane is active
pub fn handle(
    ctx: &mut ViewHandlerContext<'_>,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> HandleResult {
    match handle_search_input(code, modifiers, true) {
        SearchAction::ClearAndExit => {
            ctx.search_query.set(String::new());
            ctx.active_pane.set(Pane::List);
            HandleResult::Handled
        }
        SearchAction::Exit => {
            ctx.active_pane.set(Pane::List);
            HandleResult::Handled
        }
        SearchAction::Quit => {
            ctx.should_exit.set(true);
            HandleResult::Handled
        }
        SearchAction::Continue => HandleResult::Handled,
    }
}
