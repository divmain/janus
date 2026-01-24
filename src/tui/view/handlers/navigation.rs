//! Navigation handlers (j/k/g/G/Up/Down/PageUp/PageDown)

use iocraft::prelude::KeyCode;

use crate::tui::navigation;
use crate::tui::state::Pane;

use super::HandleResult;
use super::context::ViewHandlerContext;

/// Handle navigation keys
pub fn handle(ctx: &mut ViewHandlerContext<'_>, code: KeyCode) -> HandleResult {
    // Check if we're in the Detail pane - use dedicated detail scrolling
    if ctx.app.active_pane.get() == Pane::Detail {
        return handle_detail_navigation(ctx, code);
    }

    // List pane navigation (changes selected ticket)
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

/// Handle navigation keys when Detail pane is focused (scrolls body content)
fn handle_detail_navigation(ctx: &mut ViewHandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Char('j') | KeyCode::Down => {
            let current = ctx.data.detail_nav.scroll_offset.get();
            let new_scroll = current
                .saturating_add(1)
                .min(ctx.data.detail_nav.max_scroll);
            ctx.data.detail_nav.scroll_offset.set(new_scroll);
            HandleResult::Handled
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let current = ctx.data.detail_nav.scroll_offset.get();
            ctx.data
                .detail_nav
                .scroll_offset
                .set(current.saturating_sub(1));
            HandleResult::Handled
        }
        KeyCode::Char('g') => {
            ctx.data.detail_nav.scroll_offset.set(0);
            HandleResult::Handled
        }
        KeyCode::Char('G') => {
            // Go to bottom - set to max scrollable value
            ctx.data
                .detail_nav
                .scroll_offset
                .set(ctx.data.detail_nav.max_scroll);
            HandleResult::Handled
        }
        KeyCode::PageDown => {
            let current = ctx.data.detail_nav.scroll_offset.get();
            let page_size = ctx.data.list_height.saturating_sub(10).max(1);
            let new_scroll = current
                .saturating_add(page_size)
                .min(ctx.data.detail_nav.max_scroll);
            ctx.data.detail_nav.scroll_offset.set(new_scroll);
            HandleResult::Handled
        }
        KeyCode::PageUp => {
            let current = ctx.data.detail_nav.scroll_offset.get();
            let page_size = ctx.data.list_height.saturating_sub(10).max(1);
            ctx.data
                .detail_nav
                .scroll_offset
                .set(current.saturating_sub(page_size));
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
    let effective_height = effective_visible_height(
        ctx.data.list_nav.scroll_offset.get(),
        ctx.data.list_height,
        ctx.data.filtered_count,
    );
    let old_index = ctx.data.list_nav.selected_index.get();
    navigation::apply_scroll_down(
        ctx.data.list_nav.selected_index,
        ctx.data.list_nav.scroll_offset,
        ctx.data.filtered_count,
        effective_height,
    );
    // Reset detail scroll when ticket changes
    if ctx.data.list_nav.selected_index.get() != old_index {
        ctx.data.detail_nav.scroll_offset.set(0);
    }
}

fn handle_up(ctx: &mut ViewHandlerContext<'_>) {
    let old_index = ctx.data.list_nav.selected_index.get();
    navigation::apply_scroll_up(
        ctx.data.list_nav.selected_index,
        ctx.data.list_nav.scroll_offset,
    );
    // Reset detail scroll when ticket changes
    if ctx.data.list_nav.selected_index.get() != old_index {
        ctx.data.detail_nav.scroll_offset.set(0);
    }
}

fn handle_go_top(ctx: &mut ViewHandlerContext<'_>) {
    let old_index = ctx.data.list_nav.selected_index.get();
    navigation::apply_scroll_to_top(
        ctx.data.list_nav.selected_index,
        ctx.data.list_nav.scroll_offset,
    );
    // Reset detail scroll when ticket changes
    if ctx.data.list_nav.selected_index.get() != old_index {
        ctx.data.detail_nav.scroll_offset.set(0);
    }
}

fn handle_go_bottom(ctx: &mut ViewHandlerContext<'_>) {
    let effective_height = effective_visible_height(
        ctx.data.list_nav.scroll_offset.get(),
        ctx.data.list_height,
        ctx.data.filtered_count,
    );
    let old_index = ctx.data.list_nav.selected_index.get();
    navigation::apply_scroll_to_bottom(
        ctx.data.list_nav.selected_index,
        ctx.data.list_nav.scroll_offset,
        ctx.data.filtered_count,
        effective_height,
    );
    // Reset detail scroll when ticket changes
    if ctx.data.list_nav.selected_index.get() != old_index {
        ctx.data.detail_nav.scroll_offset.set(0);
    }
}

fn handle_page_down(ctx: &mut ViewHandlerContext<'_>) {
    let effective_height = effective_visible_height(
        ctx.data.list_nav.scroll_offset.get(),
        ctx.data.list_height,
        ctx.data.filtered_count,
    );
    let old_index = ctx.data.list_nav.selected_index.get();
    navigation::apply_page_down(
        ctx.data.list_nav.selected_index,
        ctx.data.list_nav.scroll_offset,
        ctx.data.filtered_count,
        effective_height,
    );
    // Reset detail scroll when ticket changes
    if ctx.data.list_nav.selected_index.get() != old_index {
        ctx.data.detail_nav.scroll_offset.set(0);
    }
}

fn handle_page_up(ctx: &mut ViewHandlerContext<'_>) {
    let effective_height = effective_visible_height(
        ctx.data.list_nav.scroll_offset.get(),
        ctx.data.list_height,
        ctx.data.filtered_count,
    );
    let old_index = ctx.data.list_nav.selected_index.get();
    navigation::apply_page_up(
        ctx.data.list_nav.selected_index,
        ctx.data.list_nav.scroll_offset,
        effective_height,
    );
    // Reset detail scroll when ticket changes
    if ctx.data.list_nav.selected_index.get() != old_index {
        ctx.data.detail_nav.scroll_offset.set(0);
    }
}
