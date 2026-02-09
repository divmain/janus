//! Action handlers (q, /, e, Enter, n, y)

use std::fs;

use clipboard_rs::Clipboard;
use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::tui::edit::extract_body_for_edit;

use super::HandleResult;
use super::context::BoardHandlerContext;

/// Handle action keys
pub fn handle(
    ctx: &mut BoardHandlerContext<'_>,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> HandleResult {
    match code {
        KeyCode::Char('q') if modifiers.contains(KeyModifiers::CONTROL) => {
            ctx.should_exit.set(true);
            HandleResult::Handled
        }
        KeyCode::Esc => {
            ctx.should_exit.set(true);
            HandleResult::Handled
        }
        KeyCode::Char('/') => {
            ctx.search_focused.set(true);
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

/// Edit the selected ticket - reads body content synchronously
fn handle_edit_ticket(ctx: &mut BoardHandlerContext<'_>) {
    let col = ctx.current_column.get();
    let row = ctx.current_row.get();

    if let Some(ticket) = ctx.get_ticket_at(col, row)
        && let Some(ref file_path) = ticket.file_path
    {
        // Read body content synchronously from file
        let body = fs::read_to_string(file_path)
            .ok()
            .map(|content| extract_body_for_edit(&content))
            .unwrap_or_default();

        // Set edit state directly (synchronous)
        ctx.edit_state().start_edit(ticket, body);
    }
}

/// Create a new ticket
fn handle_create_new(ctx: &mut BoardHandlerContext<'_>) {
    ctx.edit_state().start_create();
}

/// Copy the ticket ID to clipboard
fn handle_copy_ticket_id(ctx: &mut BoardHandlerContext<'_>) {
    let col = ctx.current_column.get();
    let row = ctx.current_row.get();

    if let Some(ticket) = ctx.get_ticket_at(col, row)
        && let Some(id) = &ticket.id
        && clipboard_rs::ClipboardContext::new()
            .and_then(|ctx| ctx.set_text(id.to_string()))
            .is_err()
    {
        // Silently fail if clipboard operations don't work
    }
}
