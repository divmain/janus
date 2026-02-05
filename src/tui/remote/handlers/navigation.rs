//! Navigation handlers (j/k/g/G/Up/Down)

use iocraft::prelude::KeyCode;

use crate::tui::navigation;

use super::super::state::ViewMode;
use super::context::HandlerContext;
use super::HandleResult;

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
    if ctx.view_state.detail_pane_focused() {
        handle_detail_down(ctx);
    } else if ctx.view_state.active_view() == ViewMode::Local {
        handle_local_down(ctx, shift_held);
    } else {
        handle_remote_down(ctx, shift_held);
    }
}

fn handle_up(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    if ctx.view_state.detail_pane_focused() {
        handle_detail_up(ctx);
    } else if ctx.view_state.active_view() == ViewMode::Local {
        handle_local_up(ctx, shift_held);
    } else {
        handle_remote_up(ctx, shift_held);
    }
}

fn handle_detail_down(ctx: &mut HandlerContext<'_>) {
    use crate::tui::navigation::detail_scroll_down;
    let detail_visible = ctx.view_state.show_detail();
    if !detail_visible {
        return;
    }

    if ctx.view_state.active_view() == ViewMode::Local {
        let ticket = ctx.view_data.local_tickets.read();
        let selected_idx = ctx.view_data.local_nav.selected_index();
        if let Some(metadata) = ticket.get(selected_idx)
            && let Some(file_path) = &metadata.file_path
            && let Ok(ticket_handle) = crate::ticket::Ticket::new(file_path.clone())
            && let Ok(content) = ticket_handle.read_content()
        {
            let body = crate::formatting::extract_ticket_body(&content).unwrap_or_default();
            let body_lines = body.lines().count();
            let visible_lines = 10;
            let mut scroll_data = *ctx.view_data.detail_scroll.read();
            detail_scroll_down(&mut scroll_data.local_offset, body_lines, visible_lines);
            ctx.view_data.detail_scroll.set(scroll_data);
        }
    } else {
        let issues = ctx.view_data.remote_issues.read();
        let selected_idx = ctx.view_data.remote_nav.selected_index();
        if let Some(issue) = issues.get(selected_idx) {
            let body = &issue.body;
            let body_lines = body.lines().count();
            let visible_lines = 10;
            let mut scroll_data = *ctx.view_data.detail_scroll.read();
            detail_scroll_down(&mut scroll_data.remote_offset, body_lines, visible_lines);
            ctx.view_data.detail_scroll.set(scroll_data);
        }
    }
}

fn handle_detail_up(ctx: &mut HandlerContext<'_>) {
    use crate::tui::navigation::detail_scroll_up;
    let detail_visible = ctx.view_state.show_detail();
    if !detail_visible {
        return;
    }

    if ctx.view_state.active_view() == ViewMode::Local {
        let mut scroll_data = *ctx.view_data.detail_scroll.read();
        detail_scroll_up(&mut scroll_data.local_offset);
        ctx.view_data.detail_scroll.set(scroll_data);
    } else {
        let mut scroll_data = *ctx.view_data.detail_scroll.read();
        detail_scroll_up(&mut scroll_data.remote_offset);
        ctx.view_data.detail_scroll.set(scroll_data);
    }
}

fn handle_local_down(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    if ctx.view_data.local_count == 0 {
        ctx.view_data.local_nav.set_selected_index(0);
        return;
    }

    let mut selected = ctx.view_data.local_nav.selected_index();
    let mut scroll = ctx.view_data.local_nav.scroll_offset();

    // If shift is held, extend selection to include current item
    if shift_held {
        select_local_at_index(ctx, selected);
    }

    navigation::scroll_down(
        &mut selected,
        &mut scroll,
        ctx.view_data.local_count,
        ctx.view_data.list_height,
    );
    ctx.view_data.local_nav.set_selected_index(selected);
    ctx.view_data.local_nav.set_scroll_offset(scroll);

    // Also select new item if shift is held
    if shift_held {
        let selected = ctx.view_data.local_nav.selected_index();
        select_local_at_index(ctx, selected);
    }
}

fn handle_local_up(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    let mut selected = ctx.view_data.local_nav.selected_index();
    let mut scroll = ctx.view_data.local_nav.scroll_offset();

    // If shift is held, extend selection to include current item
    if shift_held {
        select_local_at_index(ctx, selected);
    }

    navigation::scroll_up(&mut selected, &mut scroll, ctx.view_data.list_height);
    ctx.view_data.local_nav.set_selected_index(selected);
    ctx.view_data.local_nav.set_scroll_offset(scroll);

    // Also select new item if shift is held
    if shift_held {
        let selected = ctx.view_data.local_nav.selected_index();
        select_local_at_index(ctx, selected);
    }
}

fn handle_remote_down(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    if ctx.view_data.remote_count == 0 {
        ctx.view_data.remote_nav.set_selected_index(0);
        return;
    }

    let mut selected = ctx.view_data.remote_nav.selected_index();
    let mut scroll = ctx.view_data.remote_nav.scroll_offset();

    // If shift is held, extend selection to include current item
    if shift_held {
        select_remote_at_index(ctx, selected);
    }

    navigation::scroll_down(
        &mut selected,
        &mut scroll,
        ctx.view_data.remote_count,
        ctx.view_data.list_height,
    );
    ctx.view_data.remote_nav.set_selected_index(selected);
    ctx.view_data.remote_nav.set_scroll_offset(scroll);

    // Also select new item if shift is held
    if shift_held {
        let selected = ctx.view_data.remote_nav.selected_index();
        select_remote_at_index(ctx, selected);
    }
}

fn handle_remote_up(ctx: &mut HandlerContext<'_>, shift_held: bool) {
    let mut selected = ctx.view_data.remote_nav.selected_index();
    let mut scroll = ctx.view_data.remote_nav.scroll_offset();

    // If shift is held, extend selection to include current item
    if shift_held {
        select_remote_at_index(ctx, selected);
    }

    navigation::scroll_up(&mut selected, &mut scroll, ctx.view_data.list_height);
    ctx.view_data.remote_nav.set_selected_index(selected);
    ctx.view_data.remote_nav.set_scroll_offset(scroll);

    // Also select new item if shift is held
    if shift_held {
        let selected = ctx.view_data.remote_nav.selected_index();
        select_remote_at_index(ctx, selected);
    }
}

fn handle_go_top(ctx: &mut HandlerContext<'_>) {
    if ctx.view_state.detail_pane_focused() {
        let mut scroll_data = *ctx.view_data.detail_scroll.read();
        scroll_data.local_offset = 0;
        scroll_data.remote_offset = 0;
        ctx.view_data.detail_scroll.set(scroll_data);
    } else if ctx.view_state.active_view() == ViewMode::Local {
        let mut selected = ctx.view_data.local_nav.selected_index();
        let mut scroll = ctx.view_data.local_nav.scroll_offset();
        navigation::scroll_to_top(&mut selected, &mut scroll);
        ctx.view_data.local_nav.set_selected_index(selected);
        ctx.view_data.local_nav.set_scroll_offset(scroll);
    } else {
        let mut selected = ctx.view_data.remote_nav.selected_index();
        let mut scroll = ctx.view_data.remote_nav.scroll_offset();
        navigation::scroll_to_top(&mut selected, &mut scroll);
        ctx.view_data.remote_nav.set_selected_index(selected);
        ctx.view_data.remote_nav.set_scroll_offset(scroll);
    }
}

fn handle_go_bottom(ctx: &mut HandlerContext<'_>) {
    if ctx.view_state.detail_pane_focused() {
        let detail_visible = ctx.view_state.show_detail();
        if detail_visible {
            if ctx.view_state.active_view() == ViewMode::Local {
                let ticket = ctx.view_data.local_tickets.read();
                let selected_idx = ctx.view_data.local_nav.selected_index();
                if let Some(metadata) = ticket.get(selected_idx)
                    && let Some(file_path) = &metadata.file_path
                    && let Ok(ticket_handle) = crate::ticket::Ticket::new(file_path.clone())
                    && let Ok(content) = ticket_handle.read_content()
                {
                    let body = crate::formatting::extract_ticket_body(&content).unwrap_or_default();
                    let body_lines = body.lines().count();
                    let visible_lines = 10;
                    let mut scroll_data = *ctx.view_data.detail_scroll.read();
                    scroll_data.local_offset = body_lines.saturating_sub(visible_lines);
                    ctx.view_data.detail_scroll.set(scroll_data);
                }
            } else {
                let issues = ctx.view_data.remote_issues.read();
                let selected_idx = ctx.view_data.remote_nav.selected_index();
                if let Some(issue) = issues.get(selected_idx) {
                    let body = &issue.body;
                    let body_lines = body.lines().count();
                    let visible_lines = 10;
                    let mut scroll_data = *ctx.view_data.detail_scroll.read();
                    scroll_data.remote_offset = body_lines.saturating_sub(visible_lines);
                    ctx.view_data.detail_scroll.set(scroll_data);
                }
            }
        }
    } else if ctx.view_state.active_view() == ViewMode::Local {
        let mut selected = ctx.view_data.local_nav.selected_index();
        let mut scroll = ctx.view_data.local_nav.scroll_offset();
        navigation::scroll_to_bottom(
            &mut selected,
            &mut scroll,
            ctx.view_data.local_count,
            ctx.view_data.list_height,
        );
        ctx.view_data.local_nav.set_selected_index(selected);
        ctx.view_data.local_nav.set_scroll_offset(scroll);
    } else {
        let mut selected = ctx.view_data.remote_nav.selected_index();
        let mut scroll = ctx.view_data.remote_nav.scroll_offset();
        navigation::scroll_to_bottom(
            &mut selected,
            &mut scroll,
            ctx.view_data.remote_count,
            ctx.view_data.list_height,
        );
        ctx.view_data.remote_nav.set_selected_index(selected);
        ctx.view_data.remote_nav.set_scroll_offset(scroll);
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
        let mut ids = ctx.view_data.local_nav.selected_ids();
        ids.insert(id);
        ctx.view_data.local_nav.set_selected_ids(ids);
    }
}

/// Helper to select a remote issue at a given index
fn select_remote_at_index(ctx: &mut HandlerContext<'_>, idx: usize) {
    let issues = ctx.view_data.remote_issues.read();
    if let Some(issue) = issues.get(idx) {
        let id = issue.id.clone();
        drop(issues);
        let mut ids = ctx.view_data.remote_nav.selected_ids();
        ids.insert(id);
        ctx.view_data.remote_nav.set_selected_ids(ids);
    }
}
