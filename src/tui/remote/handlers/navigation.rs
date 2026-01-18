//! Navigation handlers (j/k/g/G/Up/Down)

use iocraft::prelude::KeyCode;

use crate::tui::navigation;

use super::super::state::ViewMode;
use super::HandleResult;
use super::context::HandlerContext;

/// Handle navigation keys
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode, shift_held: bool) -> HandleResult {
    match code {
        KeyCode::Char('j') | KeyCode::Char('J') | KeyCode::Down => {
            handle_down(ctx, shift_held);
            HandleResult::Handled
        }
        KeyCode::Char('k') | KeyCode::Char('K') | KeyCode::Up => {
            handle_up(ctx, shift_held);
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
        _ => HandleResult::NotHandled,
    }
}

fn handle_down(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    if ctx.active_view.get() == ViewMode::Local {
        handle_local_down(ctx, shift_held);
    } else {
        handle_remote_down(ctx, shift_held);
    }
}

fn handle_up(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    if ctx.active_view.get() == ViewMode::Local {
        handle_local_up(ctx, shift_held);
    } else {
        handle_remote_up(ctx, shift_held);
    }
}

fn handle_local_down(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    if ctx.local_count == 0 {
        ctx.local_selected_index.set(0);
        return;
    }

    let current_idx = ctx.local_selected_index.get();

    // If shift is held, extend selection to include current item
    if shift_held {
        select_local_at_index(ctx, current_idx);
    }

    let mut selected = current_idx;
    let mut scroll = ctx.local_scroll_offset.get();
    navigation::scroll_down(&mut selected, &mut scroll, ctx.local_count, ctx.list_height);
    ctx.local_selected_index.set(selected);
    ctx.local_scroll_offset.set(scroll);

    // Also select new item if shift is held
    if shift_held {
        select_local_at_index(ctx, selected);
    }
}

fn handle_local_up(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    let current_idx = ctx.local_selected_index.get();

    // If shift is held, extend selection to include current item
    if shift_held {
        select_local_at_index(ctx, current_idx);
    }

    let mut selected = current_idx;
    let mut scroll = ctx.local_scroll_offset.get();
    navigation::scroll_up(&mut selected, &mut scroll);
    ctx.local_selected_index.set(selected);
    ctx.local_scroll_offset.set(scroll);

    // Also select new item if shift is held
    if shift_held {
        select_local_at_index(ctx, selected);
    }
}

fn handle_remote_down(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    if ctx.remote_count == 0 {
        ctx.remote_selected_index.set(0);
        return;
    }

    let current_idx = ctx.remote_selected_index.get();

    // If shift is held, extend selection to include current item
    if shift_held {
        select_remote_at_index(ctx, current_idx);
    }

    let mut selected = current_idx;
    let mut scroll = ctx.remote_scroll_offset.get();
    navigation::scroll_down(
        &mut selected,
        &mut scroll,
        ctx.remote_count,
        ctx.list_height,
    );
    ctx.remote_selected_index.set(selected);
    ctx.remote_scroll_offset.set(scroll);

    // Also select new item if shift is held
    if shift_held {
        select_remote_at_index(ctx, selected);
    }
}

fn handle_remote_up(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    let current_idx = ctx.remote_selected_index.get();

    // If shift is held, extend selection to include current item
    if shift_held {
        select_remote_at_index(ctx, current_idx);
    }

    let mut selected = current_idx;
    let mut scroll = ctx.remote_scroll_offset.get();
    navigation::scroll_up(&mut selected, &mut scroll);
    ctx.remote_selected_index.set(selected);
    ctx.remote_scroll_offset.set(scroll);

    // Also select new item if shift is held
    if shift_held {
        select_remote_at_index(ctx, selected);
    }
}

fn handle_go_top(ctx: &mut HandlerContext<'_>) {
    if ctx.active_view.get() == ViewMode::Local {
        let mut selected = ctx.local_selected_index.get();
        let mut scroll = ctx.local_scroll_offset.get();
        navigation::scroll_to_top(&mut selected, &mut scroll);
        ctx.local_selected_index.set(selected);
        ctx.local_scroll_offset.set(scroll);
    } else {
        let mut selected = ctx.remote_selected_index.get();
        let mut scroll = ctx.remote_scroll_offset.get();
        navigation::scroll_to_top(&mut selected, &mut scroll);
        ctx.remote_selected_index.set(selected);
        ctx.remote_scroll_offset.set(scroll);
    }
}

fn handle_go_bottom(ctx: &mut HandlerContext<'_>) {
    if ctx.active_view.get() == ViewMode::Local {
        let mut selected = ctx.local_selected_index.get();
        let mut scroll = ctx.local_scroll_offset.get();
        navigation::scroll_to_bottom(&mut selected, &mut scroll, ctx.local_count, ctx.list_height);
        ctx.local_selected_index.set(selected);
        ctx.local_scroll_offset.set(scroll);
    } else {
        let mut selected = ctx.remote_selected_index.get();
        let mut scroll = ctx.remote_scroll_offset.get();
        navigation::scroll_to_bottom(
            &mut selected,
            &mut scroll,
            ctx.remote_count,
            ctx.list_height,
        );
        ctx.remote_selected_index.set(selected);
        ctx.remote_scroll_offset.set(scroll);
    }
}

/// Helper to select a local ticket at a given index
fn select_local_at_index(ctx: &mut HandlerContext<'_>, idx: usize) {
    let tickets = ctx.local_tickets.read();
    if let Some(ticket) = tickets.get(idx)
        && let Some(id) = &ticket.id
    {
        let id = id.clone();
        drop(tickets);
        let mut ids = ctx.local_selected_ids.read().clone();
        ids.insert(id);
        ctx.local_selected_ids.set(ids);
    }
}

/// Helper to select a remote issue at a given index
fn select_remote_at_index(ctx: &mut HandlerContext<'_>, idx: usize) {
    let issues = ctx.remote_issues.read();
    if let Some(issue) = issues.get(idx) {
        let id = issue.id.clone();
        drop(issues);
        let mut ids = ctx.remote_selected_ids.read().clone();
        ids.insert(id);
        ctx.remote_selected_ids.set(ids);
    }
}
