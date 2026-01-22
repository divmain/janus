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
    if ctx.view_state.detail_pane_focused.get() {
        handle_detail_down(ctx);
    } else if ctx.view_state.active_view.get() == ViewMode::Local {
        handle_local_down(ctx, shift_held);
    } else {
        handle_remote_down(ctx, shift_held);
    }
}

fn handle_up(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    if ctx.view_state.detail_pane_focused.get() {
        handle_detail_up(ctx);
    } else if ctx.view_state.active_view.get() == ViewMode::Local {
        handle_local_up(ctx, shift_held);
    } else {
        handle_remote_up(ctx, shift_held);
    }
}

fn handle_detail_down(ctx: &mut HandlerContext<'_>) {
    use crate::tui::navigation::apply_detail_scroll_down;
    let detail_visible = ctx.view_state.show_detail.get();
    if !detail_visible {
        return;
    }

    if ctx.view_state.active_view.get() == ViewMode::Local {
        let ticket = ctx.view_data.local_tickets.read();
        let selected_idx = ctx.view_data.local_nav.selected_index.get();
        if let Some(metadata) = ticket.get(selected_idx)
            && let Some(file_path) = &metadata.file_path
            && let Ok(ticket_handle) = crate::ticket::Ticket::new(file_path.clone())
            && let Ok(content) = ticket_handle.read_content()
        {
            let body = crate::formatting::extract_ticket_body(&content).unwrap_or_default();
            let body_lines = body.lines().count();
            let visible_lines = 10;
            apply_detail_scroll_down(ctx.view_data.local_detail_scroll_offset, body_lines, visible_lines);
        }
    } else {
        let issues = ctx.view_data.remote_issues.read();
        let selected_idx = ctx.view_data.remote_nav.selected_index.get();
        if let Some(issue) = issues.get(selected_idx) {
            let body = &issue.body;
            let body_lines = body.lines().count();
            let visible_lines = 10;
            apply_detail_scroll_down(ctx.view_data.remote_detail_scroll_offset, body_lines, visible_lines);
        }
    }
}

fn handle_detail_up(ctx: &mut HandlerContext<'_>) {
    use crate::tui::navigation::apply_detail_scroll_up;
    let detail_visible = ctx.view_state.show_detail.get();
    if !detail_visible {
        return;
    }

    if ctx.view_state.active_view.get() == ViewMode::Local {
        apply_detail_scroll_up(ctx.view_data.local_detail_scroll_offset);
    } else {
        apply_detail_scroll_up(ctx.view_data.remote_detail_scroll_offset);
    }
}

fn handle_local_down(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    if ctx.view_data.local_count == 0 {
        ctx.view_data.local_nav.selected_index.set(0);
        return;
    }

    let current_idx = ctx.view_data.local_nav.selected_index.get();

    // If shift is held, extend selection to include current item
    if shift_held {
        select_local_at_index(ctx, current_idx);
    }

    navigation::apply_scroll_down(
        ctx.view_data.local_nav.selected_index,
        ctx.view_data.local_nav.scroll_offset,
        ctx.view_data.local_count,
        ctx.view_data.list_height,
    );

    // Also select new item if shift is held
    if shift_held {
        let selected = ctx.view_data.local_nav.selected_index.get();
        select_local_at_index(ctx, selected);
    }
}

fn handle_local_up(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    let current_idx = ctx.view_data.local_nav.selected_index.get();

    // If shift is held, extend selection to include current item
    if shift_held {
        select_local_at_index(ctx, current_idx);
    }

    navigation::apply_scroll_up(
        ctx.view_data.local_nav.selected_index,
        ctx.view_data.local_nav.scroll_offset,
    );

    // Also select new item if shift is held
    if shift_held {
        let selected = ctx.view_data.local_nav.selected_index.get();
        select_local_at_index(ctx, selected);
    }
}

fn handle_remote_down(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    if ctx.view_data.remote_count == 0 {
        ctx.view_data.remote_nav.selected_index.set(0);
        return;
    }

    let current_idx = ctx.view_data.remote_nav.selected_index.get();

    // If shift is held, extend selection to include current item
    if shift_held {
        select_remote_at_index(ctx, current_idx);
    }

    navigation::apply_scroll_down(
        ctx.view_data.remote_nav.selected_index,
        ctx.view_data.remote_nav.scroll_offset,
        ctx.view_data.remote_count,
        ctx.view_data.list_height,
    );

    // Also select new item if shift is held
    if shift_held {
        let selected = ctx.view_data.remote_nav.selected_index.get();
        select_remote_at_index(ctx, selected);
    }
}

fn handle_remote_up(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    let current_idx = ctx.view_data.remote_nav.selected_index.get();

    // If shift is held, extend selection to include current item
    if shift_held {
        select_remote_at_index(ctx, current_idx);
    }

    navigation::apply_scroll_up(
        ctx.view_data.remote_nav.selected_index,
        ctx.view_data.remote_nav.scroll_offset,
    );

    // Also select new item if shift is held
    if shift_held {
        let selected = ctx.view_data.remote_nav.selected_index.get();
        select_remote_at_index(ctx, selected);
    }
}

fn handle_go_top(ctx: &mut HandlerContext<'_>) {
    if ctx.view_state.detail_pane_focused.get() {
        ctx.view_data.local_detail_scroll_offset.set(0);
        ctx.view_data.remote_detail_scroll_offset.set(0);
    } else if ctx.view_state.active_view.get() == ViewMode::Local {
        navigation::apply_scroll_to_top(
            ctx.view_data.local_nav.selected_index,
            ctx.view_data.local_nav.scroll_offset,
        );
    } else {
        navigation::apply_scroll_to_top(
            ctx.view_data.remote_nav.selected_index,
            ctx.view_data.remote_nav.scroll_offset,
        );
    }
}

fn handle_go_bottom(ctx: &mut HandlerContext<'_>) {
    if ctx.view_state.detail_pane_focused.get() {
        let detail_visible = ctx.view_state.show_detail.get();
        if detail_visible {
            if ctx.view_state.active_view.get() == ViewMode::Local {
                let ticket = ctx.view_data.local_tickets.read();
                let selected_idx = ctx.view_data.local_nav.selected_index.get();
                if let Some(metadata) = ticket.get(selected_idx)
                    && let Some(file_path) = &metadata.file_path
                    && let Ok(ticket_handle) = crate::ticket::Ticket::new(file_path.clone())
                    && let Ok(content) = ticket_handle.read_content()
                {
                    let body = crate::formatting::extract_ticket_body(&content).unwrap_or_default();
                    let body_lines = body.lines().count();
                    let visible_lines = 10;
                    ctx.view_data.local_detail_scroll_offset.set(body_lines.saturating_sub(visible_lines));
                }
            } else {
                let issues = ctx.view_data.remote_issues.read();
                let selected_idx = ctx.view_data.remote_nav.selected_index.get();
                if let Some(issue) = issues.get(selected_idx) {
                    let body = &issue.body;
                    let body_lines = body.lines().count();
                    let visible_lines = 10;
                    ctx.view_data.remote_detail_scroll_offset.set(body_lines.saturating_sub(visible_lines));
                }
            }
        }
    } else if ctx.view_state.active_view.get() == ViewMode::Local {
        navigation::apply_scroll_to_bottom(
            ctx.view_data.local_nav.selected_index,
            ctx.view_data.local_nav.scroll_offset,
            ctx.view_data.local_count,
            ctx.view_data.list_height,
        );
    } else {
        navigation::apply_scroll_to_bottom(
            ctx.view_data.remote_nav.selected_index,
            ctx.view_data.remote_nav.scroll_offset,
            ctx.view_data.remote_count,
            ctx.view_data.list_height,
        );
    }
}

/// Helper to select a local ticket at a given index
fn select_local_at_index(ctx: &mut HandlerContext<'_>, idx: usize) {
    let tickets = ctx.view_data.local_tickets.read();
    if let Some(ticket) = tickets.get(idx)
        && let Some(id) = &ticket.id
    {
        let id = id.clone();
        drop(tickets);
        let mut ids = ctx.view_data.local_nav.selected_ids.read().clone();
        ids.insert(id);
        ctx.view_data.local_nav.selected_ids.set(ids);
    }
}

/// Helper to select a remote issue at a given index
fn select_remote_at_index(ctx: &mut HandlerContext<'_>, idx: usize) {
    let issues = ctx.view_data.remote_issues.read();
    if let Some(issue) = issues.get(idx) {
        let id = issue.id.clone();
        drop(issues);
        let mut ids = ctx.view_data.remote_nav.selected_ids.read().clone();
        ids.insert(id);
        ctx.view_data.remote_nav.selected_ids.set(ids);
    }
}
