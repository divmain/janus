//! Navigation handlers (h/l/j/k/Left/Right/Up/Down/g/G/PageDown/PageUp)

use iocraft::prelude::KeyCode;

use super::context::BoardHandlerContext;
use super::HandleResult;

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
        KeyCode::Char('g') => {
            handle_go_to_top(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('G') => {
            handle_go_to_bottom(ctx);
            HandleResult::Handled
        }
        KeyCode::PageDown => {
            handle_page_down(ctx);
            HandleResult::Handled
        }
        KeyCode::PageUp => {
            handle_page_up(ctx);
            HandleResult::Handled
        }
        _ => HandleResult::NotHandled,
    }
}

/// Calculate effective visible height for a column, accounting for scroll indicators.
///
/// When there are items above or below the visible area, the column shows
/// "X more above" / "X more below" indicators that take up 1 line each.
/// The navigation logic must account for this to keep the selection visible.
fn effective_column_height(
    scroll_offset: usize,
    column_height: usize,
    total_count: usize,
) -> usize {
    if total_count == 0 || column_height == 0 {
        return column_height;
    }

    let mut effective = column_height;

    // If scrolled down, "more above" indicator takes 1 line
    if scroll_offset > 0 {
        effective = effective.saturating_sub(1);
    }

    // If there are more items below, "more below" indicator takes 1 line
    let visible_end = scroll_offset + effective;
    if visible_end < total_count {
        effective = effective.saturating_sub(1);
    }

    // Ensure we always have at least 1 visible row
    effective.max(1)
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
    let total_count = ctx.get_column_count(col);
    let max_row = total_count.saturating_sub(1);
    let new_row = (ctx.current_row.get() + 1).min(max_row);
    ctx.current_row.set(new_row);

    // Adjust scroll to keep selection visible
    let mut scroll_offsets = ctx.column_scroll_offsets.get();
    let scroll = scroll_offsets[col];
    let effective_height = effective_column_height(scroll, ctx.column_height, total_count);

    // If new row is below visible area, scroll down
    if new_row >= scroll + effective_height {
        scroll_offsets[col] = new_row.saturating_sub(effective_height - 1);
        ctx.column_scroll_offsets.set(scroll_offsets);
    }
}

/// Move up in the current column
fn handle_up(ctx: &mut BoardHandlerContext<'_>) {
    let col = ctx.current_column.get();
    let new_row = ctx.current_row.get().saturating_sub(1);
    ctx.current_row.set(new_row);

    // Adjust scroll to keep selection visible
    let mut scroll_offsets = ctx.column_scroll_offsets.get();
    let scroll = scroll_offsets[col];

    // If new row is above visible area, scroll up
    if new_row < scroll {
        scroll_offsets[col] = new_row;
        ctx.column_scroll_offsets.set(scroll_offsets);
    }
}

/// Adjust row when changing columns if current row is out of bounds
fn adjust_row_for_column(ctx: &mut BoardHandlerContext<'_>, column: usize) {
    let total_count = ctx.get_column_count(column);
    let max_row = total_count.saturating_sub(1);
    if ctx.current_row.get() > max_row {
        ctx.current_row.set(max_row);
    }

    // Adjust scroll to keep selection visible in the new column
    let mut scroll_offsets = ctx.column_scroll_offsets.get();
    let current_row = ctx.current_row.get();
    let effective_height =
        effective_column_height(scroll_offsets[column], ctx.column_height, total_count);

    // Ensure current_row is visible
    if current_row < scroll_offsets[column] {
        scroll_offsets[column] = current_row;
    } else if current_row >= scroll_offsets[column] + effective_height {
        scroll_offsets[column] = current_row.saturating_sub(effective_height - 1);
    }
    ctx.column_scroll_offsets.set(scroll_offsets);
}

/// Jump to top of current column
fn handle_go_to_top(ctx: &mut BoardHandlerContext<'_>) {
    let col = ctx.current_column.get();
    ctx.current_row.set(0);

    let mut scroll_offsets = ctx.column_scroll_offsets.get();
    scroll_offsets[col] = 0;
    ctx.column_scroll_offsets.set(scroll_offsets);
}

/// Jump to bottom of current column
fn handle_go_to_bottom(ctx: &mut BoardHandlerContext<'_>) {
    let col = ctx.current_column.get();
    let total_count = ctx.get_column_count(col);
    let max_row = total_count.saturating_sub(1);
    ctx.current_row.set(max_row);

    // Adjust scroll to show bottom
    let mut scroll_offsets = ctx.column_scroll_offsets.get();
    let effective_height =
        effective_column_height(scroll_offsets[col], ctx.column_height, total_count);
    if max_row >= effective_height {
        scroll_offsets[col] = max_row.saturating_sub(effective_height - 1);
    } else {
        scroll_offsets[col] = 0;
    }
    ctx.column_scroll_offsets.set(scroll_offsets);
}

/// Page down (half page)
fn handle_page_down(ctx: &mut BoardHandlerContext<'_>) {
    let col = ctx.current_column.get();
    let total_count = ctx.get_column_count(col);
    let max_row = total_count.saturating_sub(1);
    let effective_height = effective_column_height(
        ctx.column_scroll_offsets.get()[col],
        ctx.column_height,
        total_count,
    );
    let jump = effective_height / 2;
    let new_row = (ctx.current_row.get() + jump).min(max_row);
    ctx.current_row.set(new_row);

    // Adjust scroll
    let mut scroll_offsets = ctx.column_scroll_offsets.get();
    let effective_height =
        effective_column_height(scroll_offsets[col], ctx.column_height, total_count);
    if new_row >= scroll_offsets[col] + effective_height {
        scroll_offsets[col] = new_row.saturating_sub(effective_height - 1);
    }
    ctx.column_scroll_offsets.set(scroll_offsets);
}

/// Page up (half page)
fn handle_page_up(ctx: &mut BoardHandlerContext<'_>) {
    let col = ctx.current_column.get();
    let total_count = ctx.get_column_count(col);
    let effective_height = effective_column_height(
        ctx.column_scroll_offsets.get()[col],
        ctx.column_height,
        total_count,
    );
    let jump = effective_height / 2;
    let new_row = ctx.current_row.get().saturating_sub(jump);
    ctx.current_row.set(new_row);

    // Adjust scroll
    let mut scroll_offsets = ctx.column_scroll_offsets.get();
    if new_row < scroll_offsets[col] {
        scroll_offsets[col] = new_row;
    }
    ctx.column_scroll_offsets.set(scroll_offsets);
}
