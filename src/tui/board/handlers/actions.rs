//! Action handlers (q, /, e, Enter, n)

use iocraft::prelude::KeyCode;

use crate::tui::search::filter_tickets;
use crate::types::{TicketMetadata, TicketStatus};

use super::HandleResult;
use super::context::BoardHandlerContext;

pub use super::types::TicketAction;

/// The 5 kanban columns in order
const COLUMNS: [TicketStatus; 5] = [
    TicketStatus::New,
    TicketStatus::Next,
    TicketStatus::InProgress,
    TicketStatus::Complete,
    TicketStatus::Cancelled,
];

/// Handle action keys
pub fn handle(ctx: &mut BoardHandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Char('q') => {
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
        _ => HandleResult::NotHandled,
    }
}

/// Edit the selected ticket - sends action to the async queue
fn handle_edit_ticket(ctx: &mut BoardHandlerContext<'_>) {
    let col = ctx.current_column.get();
    let row = ctx.current_row.get();

    if let Some(ticket) = get_ticket_at(ctx, col, row)
        && let Some(id) = &ticket.id
    {
        // Send action to queue for async processing
        let _ = ctx
            .action_tx
            .send(TicketAction::LoadForEdit { id: id.clone() });
    }
}

/// Create a new ticket
fn handle_create_new(ctx: &mut BoardHandlerContext<'_>) {
    ctx.edit_state().start_create();
}

/// Get the ticket at a specific column and row
fn get_ticket_at(
    ctx: &BoardHandlerContext<'_>,
    column: usize,
    row: usize,
) -> Option<TicketMetadata> {
    if column >= COLUMNS.len() {
        return None;
    }

    let tickets_read = ctx.all_tickets.read();
    let query = ctx.search_query.to_string();
    let filtered = filter_tickets(&tickets_read, &query);
    let status = COLUMNS[column];

    let column_tickets: Vec<_> = filtered
        .iter()
        .filter(|ft| ft.ticket.status.unwrap_or_default() == status)
        .collect();

    column_tickets.get(row).map(|ft| ft.ticket.clone())
}
