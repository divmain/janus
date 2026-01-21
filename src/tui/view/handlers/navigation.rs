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

/// Calculate effective visible height, accounting for scroll indicators.
///
/// When there are items above or below the visible area, the TicketList component
/// shows "X more above" / "X more below" indicators that take up 1 line each.
/// The navigation logic must account for this to keep the selection visible.
fn effective_visible_height(scroll_offset: usize, list_height: usize, total_count: usize) -> usize {
    if total_count == 0 || list_height == 0 {
        return list_height;
    }

    let mut effective = list_height;

    // If scrolled down, "more above" indicator takes 1 line
    if scroll_offset > 0 {
        effective = effective.saturating_sub(1);
    }

    // If there are more items below, "more below" indicator takes 1 line
    // We need to estimate: will there be items below after showing `effective` rows?
    let visible_end = scroll_offset + effective;
    if visible_end < total_count {
        effective = effective.saturating_sub(1);
    }

    // Ensure we always have at least 1 visible row
    effective.max(1)
}

fn handle_down(ctx: &mut ViewHandlerContext<'_>) {
    let effective_height =
        effective_visible_height(ctx.scroll_offset.get(), ctx.list_height, ctx.filtered_count);
    navigation::apply_scroll_down(
        ctx.selected_index,
        ctx.scroll_offset,
        ctx.filtered_count,
        effective_height,
    );
}

fn handle_up(ctx: &mut ViewHandlerContext<'_>) {
    navigation::apply_scroll_up(ctx.selected_index, ctx.scroll_offset);
}

fn handle_go_top(ctx: &mut ViewHandlerContext<'_>) {
    navigation::apply_scroll_to_top(ctx.selected_index, ctx.scroll_offset);
}

fn handle_go_bottom(ctx: &mut ViewHandlerContext<'_>) {
    let effective_height =
        effective_visible_height(ctx.scroll_offset.get(), ctx.list_height, ctx.filtered_count);
    navigation::apply_scroll_to_bottom(
        ctx.selected_index,
        ctx.scroll_offset,
        ctx.filtered_count,
        effective_height,
    );
}

fn handle_page_down(ctx: &mut ViewHandlerContext<'_>) {
    let effective_height =
        effective_visible_height(ctx.scroll_offset.get(), ctx.list_height, ctx.filtered_count);
    navigation::apply_page_down(
        ctx.selected_index,
        ctx.scroll_offset,
        ctx.filtered_count,
        effective_height,
    );
}

fn handle_page_up(ctx: &mut ViewHandlerContext<'_>) {
    let effective_height =
        effective_visible_height(ctx.scroll_offset.get(), ctx.list_height, ctx.filtered_count);
    navigation::apply_page_up(ctx.selected_index, ctx.scroll_offset, effective_height);
}
