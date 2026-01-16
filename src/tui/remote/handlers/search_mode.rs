//! Search mode event handler

use iocraft::prelude::KeyCode;

use super::HandleResult;
use super::context::HandlerContext;

/// Handle events when search box is focused
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Esc => {
            ctx.search_query.set(String::new());
            ctx.search_focused.set(false);
            HandleResult::Handled
        }
        KeyCode::Enter | KeyCode::Tab => {
            ctx.search_focused.set(false);
            HandleResult::Handled
        }
        _ => {
            // Let the search box component handle other keys
            HandleResult::Handled
        }
    }
}
