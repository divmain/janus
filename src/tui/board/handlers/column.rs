//! Column toggle and status movement handlers

use iocraft::prelude::{KeyCode, State};

use crate::types::TicketStatus;

use super::HandleResult;
use super::actions::get_ticket_at;
use super::context::BoardHandlerContext;

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

/// Move ticket to next status (right) - calls async handler directly
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
        ctx.handlers.update_status.clone()((id.clone(), next_status));
    }
}

/// Move ticket to previous status (left) - calls async handler directly
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
        ctx.handlers.update_status.clone()((id.clone(), prev_status));
    }
}

/// Adjust current column to first visible column if current is hidden
pub fn adjust_column_after_toggle(current_column: &mut State<usize>, visible: &[bool; 5]) {
    let current = current_column.get();
    if !visible[current]
        && let Some(first_visible) = visible.iter().position(|&v| v)
    {
        current_column.set(first_visible);
    }
}
