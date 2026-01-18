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
    let mut selected = ctx.selected_index.get();
    let mut scroll = ctx.scroll_offset.get();
    navigation::scroll_down(
        &mut selected,
        &mut scroll,
        ctx.filtered_count,
        ctx.list_height,
    );
    ctx.selected_index.set(selected);
    ctx.scroll_offset.set(scroll);
}

fn handle_up(ctx: &mut ViewHandlerContext<'_>) {
    let mut selected = ctx.selected_index.get();
    let mut scroll = ctx.scroll_offset.get();
    navigation::scroll_up(&mut selected, &mut scroll);
    ctx.selected_index.set(selected);
    ctx.scroll_offset.set(scroll);
}

fn handle_go_top(ctx: &mut ViewHandlerContext<'_>) {
    let mut selected = ctx.selected_index.get();
    let mut scroll = ctx.scroll_offset.get();
    navigation::scroll_to_top(&mut selected, &mut scroll);
    ctx.selected_index.set(selected);
    ctx.scroll_offset.set(scroll);
}

fn handle_go_bottom(ctx: &mut ViewHandlerContext<'_>) {
    let mut selected = ctx.selected_index.get();
    let mut scroll = ctx.scroll_offset.get();
    navigation::scroll_to_bottom(
        &mut selected,
        &mut scroll,
        ctx.filtered_count,
        ctx.list_height,
    );
    ctx.selected_index.set(selected);
    ctx.scroll_offset.set(scroll);
}

fn handle_page_down(ctx: &mut ViewHandlerContext<'_>) {
    let mut selected = ctx.selected_index.get();
    let mut scroll = ctx.scroll_offset.get();
    navigation::page_down(
        &mut selected,
        &mut scroll,
        ctx.filtered_count,
        ctx.list_height,
    );
    ctx.selected_index.set(selected);
    ctx.scroll_offset.set(scroll);
}

fn handle_page_up(ctx: &mut ViewHandlerContext<'_>) {
    let mut selected = ctx.selected_index.get();
    let mut scroll = ctx.scroll_offset.get();
    navigation::page_up(&mut selected, &mut scroll, ctx.list_height);
    ctx.selected_index.set(selected);
    ctx.scroll_offset.set(scroll);
}
