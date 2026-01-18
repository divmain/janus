//! Navigation handlers (h/l/j/k/Left/Right/Up/Down)

use iocraft::prelude::KeyCode;

use super::HandleResult;
use super::context::BoardHandlerContext;
use crate::tui::search::filter_tickets;

/// Handle navigation keys
pub fn handle(ctx: &mut BoardHandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Char('h') | KeyCode::Left => {
            handle_left(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('l') | KeyCode::Right => {
            handle_right(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('j') | KeyCode::Down => {
            handle_down(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('k') | KeyCode::Up => {
            handle_up(ctx);
            HandleResult::Handled
        }
        _ => HandleResult::NotHandled,
    }
}

/// Move to the previous visible column (left)
fn handle_left(ctx: &mut BoardHandlerContext<'_>) {
    let vis = ctx.visible_columns.get();
    let visible_idx: Vec<usize> = vis
        .iter()
        .enumerate()
        .filter_map(|(i, &v)| if v { Some(i) } else { None })
        .collect();

    if visible_idx.is_empty() {
        return;
    }

    let curr_pos = visible_idx
        .iter()
        .position(|&i| i == ctx.current_column.get())
        .unwrap_or(0);

    if curr_pos > 0 {
        let new_col = visible_idx[curr_pos - 1];
        ctx.current_column.set(new_col);
        adjust_row_for_column(ctx, new_col);
    }
}

/// Move to the next visible column (right)
fn handle_right(ctx: &mut BoardHandlerContext<'_>) {
    let vis = ctx.visible_columns.get();
    let visible_idx: Vec<usize> = vis
        .iter()
        .enumerate()
        .filter_map(|(i, &v)| if v { Some(i) } else { None })
        .collect();

    if visible_idx.is_empty() {
        return;
    }

    let curr_pos = visible_idx
        .iter()
        .position(|&i| i == ctx.current_column.get())
        .unwrap_or(0);

    if curr_pos < visible_idx.len() - 1 {
        let new_col = visible_idx[curr_pos + 1];
        ctx.current_column.set(new_col);
        adjust_row_for_column(ctx, new_col);
    }
}

/// Move down in the current column
fn handle_down(ctx: &mut BoardHandlerContext<'_>) {
    let col = ctx.current_column.get();
    let max_row = get_column_count(ctx, col).saturating_sub(1);
    let new_row = (ctx.current_row.get() + 1).min(max_row);
    ctx.current_row.set(new_row);
}

/// Move up in the current column
fn handle_up(ctx: &mut BoardHandlerContext<'_>) {
    let new_row = ctx.current_row.get().saturating_sub(1);
    ctx.current_row.set(new_row);
}

/// Adjust row when changing columns if current row is out of bounds
fn adjust_row_for_column(ctx: &mut BoardHandlerContext<'_>, column: usize) {
    let max_row = get_column_count(ctx, column).saturating_sub(1);
    if ctx.current_row.get() > max_row {
        ctx.current_row.set(max_row);
    }
}

/// Get the number of tickets in a column
fn get_column_count(ctx: &BoardHandlerContext<'_>, column: usize) -> usize {
    use crate::types::TicketStatus;

    const COLUMNS: [TicketStatus; 5] = [
        TicketStatus::New,
        TicketStatus::Next,
        TicketStatus::InProgress,
        TicketStatus::Complete,
        TicketStatus::Cancelled,
    ];

    if column >= COLUMNS.len() {
        return 0;
    }

    let tickets_read = ctx.all_tickets.read();
    let query = ctx.search_query.to_string();
    let filtered = filter_tickets(&tickets_read, &query);
    let status = COLUMNS[column];

    filtered
        .iter()
        .filter(|ft| ft.ticket.status.unwrap_or_default() == status)
        .count()
}
