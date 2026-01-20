//! Navigation handlers (j/k/g/G/Up/Down/PageUp/PageDown)

use iocraft::prelude::KeyCode;

use crate::tui::navigation;

use super::HandleResult;
use super::context::ViewHandlerContext;

/// Handle navigation keys
pub fn handle(ctx: &mut ViewHandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Char('j') | KeyCode::Down => {
            handle_down(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('k') | KeyCode::Up => {
            handle_up(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('g') => {
            handle_go_top(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('G') => {
            handle_go_bottom(ctx);
            HandleResult::Handled
        }
        KeyCode::PageDown => {
            handle_page_down(ctx);
            HandleResult::Handled
        }
        KeyCode::PageUp => {
            handle_page_up(ctx);
            HandleResult::Handled
        }
        _ => HandleResult::NotHandled,
    }
}

fn handle_down(ctx: &mut ViewHandlerContext<'_>) {
    navigation::apply_scroll_down(
        ctx.selected_index,
        ctx.scroll_offset,
        ctx.filtered_count,
        ctx.list_height,
    );
}

fn handle_up(ctx: &mut ViewHandlerContext<'_>) {
    navigation::apply_scroll_up(ctx.selected_index, ctx.scroll_offset);
}

fn handle_go_top(ctx: &mut ViewHandlerContext<'_>) {
    navigation::apply_scroll_to_top(ctx.selected_index, ctx.scroll_offset);
}

fn handle_go_bottom(ctx: &mut ViewHandlerContext<'_>) {
    navigation::apply_scroll_to_bottom(
        ctx.selected_index,
        ctx.scroll_offset,
        ctx.filtered_count,
        ctx.list_height,
    );
}

fn handle_page_down(ctx: &mut ViewHandlerContext<'_>) {
    navigation::apply_page_down(
        ctx.selected_index,
        ctx.scroll_offset,
        ctx.filtered_count,
        ctx.list_height,
    );
}

fn handle_page_up(ctx: &mut ViewHandlerContext<'_>) {
    navigation::apply_page_up(ctx.selected_index, ctx.scroll_offset, ctx.list_height);
}
