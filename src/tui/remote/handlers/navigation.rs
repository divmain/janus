//! Navigation handlers (j/k/g/G/Up/Down)

use iocraft::prelude::KeyCode;

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
    let new_idx = (current_idx + 1).min(ctx.local_count - 1);

    // If shift is held, extend selection to include current item
    if shift_held {
        select_local_at_index(ctx, current_idx);
    }

    ctx.local_selected_index.set(new_idx);

    // Scroll if needed
    if new_idx >= ctx.local_scroll_offset.get() + ctx.list_height {
        ctx.local_scroll_offset
            .set(new_idx.saturating_sub(ctx.list_height - 1));
    }

    // Also select new item if shift is held
    if shift_held {
        select_local_at_index(ctx, new_idx);
    }
}

fn handle_local_up(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    let current_idx = ctx.local_selected_index.get();
    let new_idx = current_idx.saturating_sub(1);

    // If shift is held, extend selection to include current item
    if shift_held {
        select_local_at_index(ctx, current_idx);
    }

    ctx.local_selected_index.set(new_idx);

    // Scroll if needed
    if new_idx < ctx.local_scroll_offset.get() {
        ctx.local_scroll_offset.set(new_idx);
    }

    // Also select new item if shift is held
    if shift_held {
        select_local_at_index(ctx, new_idx);
    }
}

fn handle_remote_down(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    if ctx.remote_count == 0 {
        ctx.remote_selected_index.set(0);
        return;
    }

    let current_idx = ctx.remote_selected_index.get();
    let new_idx = (current_idx + 1).min(ctx.remote_count - 1);

    // If shift is held, extend selection to include current item
    if shift_held {
        select_remote_at_index(ctx, current_idx);
    }

    ctx.remote_selected_index.set(new_idx);

    // Scroll if needed
    if new_idx >= ctx.remote_scroll_offset.get() + ctx.list_height {
        ctx.remote_scroll_offset
            .set(new_idx.saturating_sub(ctx.list_height - 1));
    }

    // Also select new item if shift is held
    if shift_held {
        select_remote_at_index(ctx, new_idx);
    }
}

fn handle_remote_up(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    let current_idx = ctx.remote_selected_index.get();
    let new_idx = current_idx.saturating_sub(1);

    // If shift is held, extend selection to include current item
    if shift_held {
        select_remote_at_index(ctx, current_idx);
    }

    ctx.remote_selected_index.set(new_idx);

    // Scroll if needed
    if new_idx < ctx.remote_scroll_offset.get() {
        ctx.remote_scroll_offset.set(new_idx);
    }

    // Also select new item if shift is held
    if shift_held {
        select_remote_at_index(ctx, new_idx);
    }
}

fn handle_go_top(ctx: &mut HandlerContext<'_>) {
    if ctx.active_view.get() == ViewMode::Local {
        ctx.local_selected_index.set(0);
        ctx.local_scroll_offset.set(0);
    } else {
        ctx.remote_selected_index.set(0);
        ctx.remote_scroll_offset.set(0);
    }
}

fn handle_go_bottom(ctx: &mut HandlerContext<'_>) {
    if ctx.active_view.get() == ViewMode::Local {
        if ctx.local_count > 0 {
            let new_idx = ctx.local_count - 1;
            ctx.local_selected_index.set(new_idx);
            if new_idx >= ctx.list_height {
                ctx.local_scroll_offset
                    .set(new_idx.saturating_sub(ctx.list_height - 1));
            }
        }
    } else if ctx.remote_count > 0 {
        let new_idx = ctx.remote_count - 1;
        ctx.remote_selected_index.set(new_idx);
        if new_idx >= ctx.list_height {
            ctx.remote_scroll_offset
                .set(new_idx.saturating_sub(ctx.list_height - 1));
        }
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
