//! List and Detail mode action handlers

use std::fs;

use clipboard_rs::Clipboard;
use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::tui::edit::extract_body_for_edit;
use crate::tui::state::Pane;

use super::HandleResult;
use super::context::ViewHandlerContext;

/// Handle events when list pane is active
pub fn handle_list(
    ctx: &mut ViewHandlerContext<'_>,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> HandleResult {
    if ctx.app.is_triage_mode {
        return handle_list_triage(ctx, code, modifiers);
    }

    match code {
        KeyCode::Char('q') if modifiers.contains(KeyModifiers::CONTROL) => {
            ctx.app.should_exit.set(true);
            HandleResult::Handled
        }
        KeyCode::Esc => {
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
        KeyCode::Char('y') => {
            handle_copy_ticket_id(ctx);
            HandleResult::Handled
        }
        _ => HandleResult::NotHandled,
    }
}

/// Handle events when detail pane is active
pub fn handle_detail(
    ctx: &mut ViewHandlerContext<'_>,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> HandleResult {
    if ctx.app.is_triage_mode {
        return handle_detail_triage(ctx, code, modifiers);
    }

    match code {
        KeyCode::Char('q') if modifiers.contains(KeyModifiers::CONTROL) => {
            ctx.app.should_exit.set(true);
            HandleResult::Handled
        }
        KeyCode::Esc => {
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
        KeyCode::Char('e') | KeyCode::Enter => {
            handle_edit_ticket(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('n') => {
            handle_create_new(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('s') => {
            handle_cycle_status(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('y') => {
            handle_copy_ticket_id(ctx);
            HandleResult::Handled
        }
        _ => HandleResult::NotHandled,
    }
}

/// Handle list pane events in triage mode
///
/// Note: `n` (note) and `c` (cancel) keys are handled at the component level
/// to show modals before executing actions.
fn handle_list_triage(
    ctx: &mut ViewHandlerContext<'_>,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> HandleResult {
    match code {
        KeyCode::Char('q') if modifiers.contains(KeyModifiers::CONTROL) => {
            ctx.app.should_exit.set(true);
            HandleResult::Handled
        }
        KeyCode::Esc => {
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
fn handle_detail_triage(
    ctx: &mut ViewHandlerContext<'_>,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> HandleResult {
    match code {
        KeyCode::Char('q') if modifiers.contains(KeyModifiers::CONTROL) => {
            ctx.app.should_exit.set(true);
            HandleResult::Handled
        }
        KeyCode::Esc => {
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

/// Cycle status for selected ticket - calls async handler directly
fn handle_cycle_status(ctx: &mut ViewHandlerContext<'_>) {
    if let Some(ft) = ctx
        .data
        .filtered_tickets
        .get(ctx.data.list_nav.selected_index.get())
        && let Some(id) = &ft.ticket.id
    {
        ctx.handlers.cycle_status.clone()(id.to_string());
    }
}

/// Edit the selected ticket
fn handle_edit_ticket(ctx: &mut ViewHandlerContext<'_>) {
    if let Some(ft) = ctx
        .data
        .filtered_tickets
        .get(ctx.data.list_nav.selected_index.get())
    {
        // Read body content synchronously from file
        let body = ft
            .ticket
            .file_path
            .as_ref()
            .and_then(|path| fs::read_to_string(path).ok())
            .map(|content| extract_body_for_edit(&content))
            .unwrap_or_default();

        // Set edit state directly (synchronous, like handle_create_new)
        ctx.edit_form_state()
            .start_edit(ft.ticket.as_ref().clone(), body);
    }
}

/// Create a new ticket
fn handle_create_new(ctx: &mut ViewHandlerContext<'_>) {
    ctx.edit_form_state().start_create();
}

/// Mark selected ticket as triaged - calls async handler directly
fn handle_mark_triaged(ctx: &mut ViewHandlerContext<'_>) {
    if let Some(ft) = ctx
        .data
        .filtered_tickets
        .get(ctx.data.list_nav.selected_index.get())
        && let Some(id) = &ft.ticket.id
    {
        ctx.handlers.mark_triaged.clone()((id.to_string(), true));
    }
}

/// Copy the ticket ID to clipboard
fn handle_copy_ticket_id(ctx: &mut ViewHandlerContext<'_>) {
    if let Some(ft) = ctx
        .data
        .filtered_tickets
        .get(ctx.data.list_nav.selected_index.get())
        && let Some(id) = &ft.ticket.id
        && clipboard_rs::ClipboardContext::new()
            .and_then(|ctx| ctx.set_text(id.to_string()))
            .is_err()
    {
        // Silently fail if clipboard operations don't work
    }
}
