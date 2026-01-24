//! List and Detail mode action handlers

use iocraft::prelude::KeyCode;

use crate::tui::state::Pane;

use super::HandleResult;
use super::context::ViewHandlerContext;
use super::types::ViewAction;

/// Handle events when list pane is active
pub fn handle_list(ctx: &mut ViewHandlerContext<'_>, code: KeyCode) -> HandleResult {
    if ctx.app.is_triage_mode {
        return handle_list_triage(ctx, code);
    }

    match code {
        KeyCode::Char('q') => {
            ctx.app.should_exit.set(true);
            HandleResult::Handled
        }
        KeyCode::Char('/') => {
            ctx.app.active_pane.set(Pane::Search);
            HandleResult::Handled
        }
        KeyCode::Tab => {
            ctx.app.active_pane.set(Pane::Detail);
            HandleResult::Handled
        }
        KeyCode::Char('s') => {
            handle_cycle_status(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('e') | KeyCode::Enter => {
            handle_edit_ticket(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('n') => {
            handle_create_new(ctx);
            HandleResult::Handled
        }
        _ => HandleResult::NotHandled,
    }
}

/// Handle events when detail pane is active
pub fn handle_detail(ctx: &mut ViewHandlerContext<'_>, code: KeyCode) -> HandleResult {
    if ctx.app.is_triage_mode {
        return handle_detail_triage(ctx, code);
    }

    match code {
        KeyCode::Char('q') => {
            ctx.app.should_exit.set(true);
            HandleResult::Handled
        }
        KeyCode::Tab | KeyCode::Esc => {
            ctx.app.active_pane.set(Pane::List);
            HandleResult::Handled
        }
        KeyCode::Char('/') => {
            ctx.app.active_pane.set(Pane::Search);
            HandleResult::Handled
        }
        KeyCode::Char('e') | KeyCode::Enter => {
            handle_edit_ticket(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('n') => {
            handle_create_new(ctx);
            HandleResult::Handled
        }
        _ => HandleResult::NotHandled,
    }
}

/// Handle list pane events in triage mode
///
/// Note: `n` (note) and `c` (cancel) keys are handled at the component level
/// to show modals before executing actions.
fn handle_list_triage(ctx: &mut ViewHandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => {
            ctx.app.should_exit.set(true);
            HandleResult::Handled
        }
        KeyCode::Char('/') => {
            ctx.app.active_pane.set(Pane::Search);
            HandleResult::Handled
        }
        KeyCode::Tab => {
            ctx.app.active_pane.set(Pane::Detail);
            HandleResult::Handled
        }
        KeyCode::Char('t') => {
            handle_mark_triaged(ctx);
            HandleResult::Handled
        }
        // Note: 'c' key is handled at component level to show confirmation modal
        // Note: 'n' key is handled at component level to show note input modal
        _ => HandleResult::NotHandled,
    }
}

/// Handle detail pane events in triage mode
///
/// Note: `n` (note) and `c` (cancel) keys are handled at the component level
/// to show modals before executing actions.
fn handle_detail_triage(ctx: &mut ViewHandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => {
            ctx.app.should_exit.set(true);
            HandleResult::Handled
        }
        KeyCode::Tab => {
            ctx.app.active_pane.set(Pane::List);
            HandleResult::Handled
        }
        KeyCode::Char('/') => {
            ctx.app.active_pane.set(Pane::Search);
            HandleResult::Handled
        }
        KeyCode::Char('t') => {
            handle_mark_triaged(ctx);
            HandleResult::Handled
        }
        // Note: 'c' key is handled at component level to show confirmation modal
        // Note: 'n' key is handled at component level to show note input modal
        _ => HandleResult::NotHandled,
    }
}

/// Cycle status for selected ticket
fn handle_cycle_status(ctx: &mut ViewHandlerContext<'_>) {
    if let Some(ft) = ctx
        .data
        .filtered_tickets
        .get(ctx.data.list_nav.selected_index.get())
        && let Some(id) = &ft.ticket.id
    {
        _ = ctx
            .actions
            .tx
            .send(ViewAction::CycleStatus { id: id.clone() });
    }
}

/// Edit the selected ticket
fn handle_edit_ticket(ctx: &mut ViewHandlerContext<'_>) {
    if let Some(ft) = ctx
        .data
        .filtered_tickets
        .get(ctx.data.list_nav.selected_index.get())
        && let Some(id) = &ft.ticket.id
    {
        _ = ctx
            .actions
            .tx
            .send(ViewAction::LoadForEdit { id: id.clone() });
    }
}

/// Create a new ticket
fn handle_create_new(ctx: &mut ViewHandlerContext<'_>) {
    ctx.edit.editing_ticket_id.set(String::new());
    ctx.edit_form_state().start_create();
}

/// Mark selected ticket as triaged
fn handle_mark_triaged(ctx: &mut ViewHandlerContext<'_>) {
    if let Some(ft) = ctx
        .data
        .filtered_tickets
        .get(ctx.data.list_nav.selected_index.get())
        && let Some(id) = &ft.ticket.id
    {
        _ = ctx.actions.tx.send(ViewAction::MarkTriaged {
            id: id.clone(),
            triaged: true,
        });
    }
}
