//! Column toggle and status movement handlers

use iocraft::prelude::{KeyCode, State};

use crate::tui::search::filter_tickets;
use crate::types::{TicketMetadata, TicketStatus};

use super::HandleResult;
use super::context::BoardHandlerContext;
use super::super::BoardAction;

/// The 5 kanban columns in order
const COLUMNS: [TicketStatus; 5] = [
    TicketStatus::New,
    TicketStatus::Next,
    TicketStatus::InProgress,
    TicketStatus::Complete,
    TicketStatus::Cancelled,
];

/// Handle column toggle keys (1-5)
pub fn handle_toggle(ctx: &mut BoardHandlerContext<'_>, code: KeyCode) -> HandleResult {
    let column_index = match code {
        KeyCode::Char('1') => Some(0),
        KeyCode::Char('2') => Some(1),
        KeyCode::Char('3') => Some(2),
        KeyCode::Char('4') => Some(3),
        KeyCode::Char('5') => Some(4),
        _ => None,
    };

    if let Some(idx) = column_index {
        let mut vis = ctx.visible_columns.get();
        vis[idx] = !vis[idx];
        ctx.visible_columns.set(vis);
        adjust_column_after_toggle(ctx.current_column, &vis);
        HandleResult::Handled
    } else {
        HandleResult::NotHandled
    }
}

/// Handle status movement keys (s/S)
pub fn handle_status_move(ctx: &mut BoardHandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Char('s') => {
            handle_move_right(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('S') => {
            handle_move_left(ctx);
            HandleResult::Handled
        }
        _ => HandleResult::NotHandled,
    }
}

/// Move ticket to next status (right) - sends action to async queue
fn handle_move_right(ctx: &mut BoardHandlerContext<'_>) {
    let col = ctx.current_column.get();
    let row = ctx.current_row.get();

    if col >= COLUMNS.len() - 1 {
        return;
    }

    if let Some(ticket) = get_ticket_at(ctx, col, row)
        && let Some(id) = &ticket.id
    {
        let next_status = COLUMNS[col + 1];
        let _ = ctx.action_tx.tx.send(BoardAction::UpdateStatus {
            id: id.clone(),
            status: next_status,
        });
    }
}

/// Move ticket to previous status (left) - sends action to async queue
fn handle_move_left(ctx: &mut BoardHandlerContext<'_>) {
    let col = ctx.current_column.get();
    let row = ctx.current_row.get();

    if col == 0 {
        return;
    }

    if let Some(ticket) = get_ticket_at(ctx, col, row)
        && let Some(id) = &ticket.id
    {
        let prev_status = COLUMNS[col - 1];
        let _ = ctx.action_tx.tx.send(BoardAction::UpdateStatus {
            id: id.clone(),
            status: prev_status,
        });
    }
}

/// Adjust current column to first visible column if current is hidden
pub fn adjust_column_after_toggle(current_column: &mut State<usize>, visible: &[bool; 5]) {
    let current = current_column.get();
    if !visible[current] {
        if let Some(first_visible) = visible.iter().position(|&v| v) {
            current_column.set(first_visible);
        }
    }
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
