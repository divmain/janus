//! Kanban board view (`janus board`)
//!
//! Provides an interactive TUI for viewing and managing tickets organized
//! by status in a kanban-style board layout with columns for each status.

use iocraft::prelude::*;

use crate::ticket::Ticket;
use crate::tui::components::{
    EmptyState, EmptyStateKind, Footer, InlineSearchBox, TicketCard, board_shortcuts,
    edit_shortcuts, empty_shortcuts,
};
use crate::tui::edit::{EditForm, EditResult, extract_body_for_edit};
use crate::tui::search::{FilteredTicket, filter_tickets};
use crate::tui::state::{InitResult, TuiState};
use crate::tui::theme::theme;
use crate::types::{TicketMetadata, TicketStatus};

/// The 5 kanban columns in order
const COLUMNS: [TicketStatus; 5] = [
    TicketStatus::New,
    TicketStatus::Next,
    TicketStatus::InProgress,
    TicketStatus::Complete,
    TicketStatus::Cancelled,
];

/// Column display names
const COLUMN_NAMES: [&str; 5] = ["NEW", "NEXT", "IN PROGRESS", "COMPLETE", "CANCELLED"];

/// Column toggle keys for header display
const COLUMN_KEYS: [char; 5] = ['N', 'X', 'I', 'C', '_'];

/// Props for the KanbanBoard component
#[derive(Default, Props)]
pub struct KanbanBoardProps {}

/// Get tickets for a specific column from the filtered list
fn get_column_tickets(filtered: &[FilteredTicket], status: TicketStatus) -> Vec<FilteredTicket> {
    filtered
        .iter()
        .filter(|ft| ft.ticket.status.unwrap_or_default() == status)
        .cloned()
        .collect()
}

/// Get the ticket at a specific column and row
fn get_ticket_at(
    all_tickets: &[TicketMetadata],
    query: &str,
    column: usize,
    row: usize,
) -> Option<TicketMetadata> {
    let filtered = filter_tickets(all_tickets, query);
    if column < COLUMNS.len() {
        let column_tickets = get_column_tickets(&filtered, COLUMNS[column]);
        column_tickets.get(row).map(|ft| ft.ticket.clone())
    } else {
        None
    }
}

/// Get the number of tickets in a column
fn get_column_count(all_tickets: &[TicketMetadata], query: &str, column: usize) -> usize {
    if column >= COLUMNS.len() {
        return 0;
    }
    let filtered = filter_tickets(all_tickets, query);
    get_column_tickets(&filtered, COLUMNS[column]).len()
}

/// Main kanban board component
///
/// Layout:
/// ```text
/// +------------------------------------------+
/// | Header                      [N][X][I][C] |
/// +------------------------------------------+
/// | / search...                              |
/// +--------+--------+--------+--------+------+
/// |  NEW   |  NEXT  |  WIP   |  DONE  | CAN  |
/// |   3    |   1    |   2    |   5    |  1   |
/// +--------+--------+--------+--------+------+
/// | Card1  | Card1  | Card1  | Card1  | Card |
/// | Card2  | ...    | Card2  | Card2  | ...  |
/// | Card3  |        |        | ...    |      |
/// +--------+--------+--------+--------+------+
/// | Footer with shortcuts                    |
/// +------------------------------------------+
/// ```
#[component]
pub fn KanbanBoard<'a>(_props: &KanbanBoardProps, mut hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let (width, height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();

    // State management
    let init_result: State<InitResult> = hooks.use_state(|| TuiState::init().1);
    let mut all_tickets: State<Vec<TicketMetadata>> =
        hooks.use_state(|| TuiState::new().all_tickets);
    let mut search_query = hooks.use_state(String::new);
    let mut should_exit = hooks.use_state(|| false);
    let mut needs_reload = hooks.use_state(|| false);

    // Column visibility state (all visible by default)
    let mut visible_columns = hooks.use_state(|| [true; 5]);

    // Navigation state
    let mut current_column = hooks.use_state(|| 0usize);
    let mut current_row = hooks.use_state(|| 0usize);
    let mut search_focused = hooks.use_state(|| false);

    // Edit form state
    let mut edit_result: State<EditResult> = hooks.use_state(EditResult::default);
    let mut is_editing_existing = hooks.use_state(|| false);
    let mut is_creating_new = hooks.use_state(|| false);
    let mut editing_ticket: State<TicketMetadata> = hooks.use_state(TicketMetadata::default);
    let mut editing_body = hooks.use_state(String::new);

    // Reload tickets if needed
    if needs_reload.get() {
        needs_reload.set(false);
        all_tickets.set(TuiState::new().all_tickets);
    }

    // Handle edit form result
    match edit_result.get() {
        EditResult::Saved => {
            edit_result.set(EditResult::Editing);
            is_editing_existing.set(false);
            is_creating_new.set(false);
            editing_ticket.set(TicketMetadata::default());
            editing_body.set(String::new());
            needs_reload.set(true);
        }
        EditResult::Cancelled => {
            edit_result.set(EditResult::Editing);
            is_editing_existing.set(false);
            is_creating_new.set(false);
            editing_ticket.set(TicketMetadata::default());
            editing_body.set(String::new());
        }
        EditResult::Editing => {}
    }

    let is_editing = is_editing_existing.get() || is_creating_new.get();

    // Filter tickets by search query for rendering
    let query_str = search_query.to_string();
    let tickets_ref = all_tickets.read();
    let filtered: Vec<FilteredTicket> = filter_tickets(&tickets_ref, &query_str);
    drop(tickets_ref);

    // Group filtered tickets by status for rendering
    let tickets_by_status: Vec<Vec<FilteredTicket>> = COLUMNS
        .iter()
        .map(|status| get_column_tickets(&filtered, *status))
        .collect();

    // Get visible column indices
    let visible_indices: Vec<usize> = visible_columns
        .get()
        .iter()
        .enumerate()
        .filter_map(|(i, &v)| if v { Some(i) } else { None })
        .collect();

    // Calculate column heights
    let available_height = height.saturating_sub(6); // header + search + column headers + footer

    // Keyboard event handling
    hooks.use_terminal_events({
        move |event| {
            // Skip if edit form is open
            if is_editing {
                return;
            }

            match event {
                TerminalEvent::Key(KeyEvent {
                    code,
                    kind,
                    modifiers,
                    ..
                }) if kind != KeyEventKind::Release => {
                    // Search mode handling
                    if search_focused.get() {
                        match code {
                            KeyCode::Esc => {
                                search_query.set(String::new());
                                search_focused.set(false);
                            }
                            KeyCode::Enter | KeyCode::Tab => {
                                search_focused.set(false);
                            }
                            KeyCode::Char('q') if modifiers.contains(KeyModifiers::CONTROL) => {
                                should_exit.set(true);
                            }
                            _ => {}
                        }
                        return;
                    }

                    // Board mode handling
                    match code {
                        KeyCode::Char('q') => {
                            should_exit.set(true);
                        }
                        KeyCode::Char('/') => {
                            search_focused.set(true);
                        }
                        // Column navigation (left/right)
                        KeyCode::Char('h') | KeyCode::Left => {
                            let vis = visible_columns.get();
                            let visible_idx: Vec<usize> = vis
                                .iter()
                                .enumerate()
                                .filter_map(|(i, &v)| if v { Some(i) } else { None })
                                .collect();
                            if !visible_idx.is_empty() {
                                let curr_pos = visible_idx
                                    .iter()
                                    .position(|&i| i == current_column.get())
                                    .unwrap_or(0);
                                if curr_pos > 0 {
                                    let new_col = visible_idx[curr_pos - 1];
                                    current_column.set(new_col);
                                    // Reset row if out of bounds
                                    let tickets_read = all_tickets.read();
                                    let query = search_query.to_string();
                                    let max_row = get_column_count(&tickets_read, &query, new_col)
                                        .saturating_sub(1);
                                    drop(tickets_read);
                                    if current_row.get() > max_row {
                                        current_row.set(max_row);
                                    }
                                }
                            }
                        }
                        KeyCode::Char('l') | KeyCode::Right => {
                            let vis = visible_columns.get();
                            let visible_idx: Vec<usize> = vis
                                .iter()
                                .enumerate()
                                .filter_map(|(i, &v)| if v { Some(i) } else { None })
                                .collect();
                            if !visible_idx.is_empty() {
                                let curr_pos = visible_idx
                                    .iter()
                                    .position(|&i| i == current_column.get())
                                    .unwrap_or(0);
                                if curr_pos < visible_idx.len() - 1 {
                                    let new_col = visible_idx[curr_pos + 1];
                                    current_column.set(new_col);
                                    // Reset row if out of bounds
                                    let tickets_read = all_tickets.read();
                                    let query = search_query.to_string();
                                    let max_row = get_column_count(&tickets_read, &query, new_col)
                                        .saturating_sub(1);
                                    drop(tickets_read);
                                    if current_row.get() > max_row {
                                        current_row.set(max_row);
                                    }
                                }
                            }
                        }
                        // Card navigation (up/down)
                        KeyCode::Char('j') | KeyCode::Down => {
                            let col = current_column.get();
                            let tickets_read = all_tickets.read();
                            let query = search_query.to_string();
                            let max_row =
                                get_column_count(&tickets_read, &query, col).saturating_sub(1);
                            drop(tickets_read);
                            let new_row = (current_row.get() + 1).min(max_row);
                            current_row.set(new_row);
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            let new_row = current_row.get().saturating_sub(1);
                            current_row.set(new_row);
                        }
                        // Move ticket right (next status)
                        KeyCode::Char('s') => {
                            let col = current_column.get();
                            let row = current_row.get();
                            if col < COLUMNS.len() - 1 {
                                let tickets_read = all_tickets.read();
                                let query = search_query.to_string();
                                if let Some(ticket) = get_ticket_at(&tickets_read, &query, col, row)
                                {
                                    drop(tickets_read);
                                    if let Some(id) = &ticket.id {
                                        let next_status = COLUMNS[col + 1];
                                        if let Ok(ticket_handle) = Ticket::find(id) {
                                            let _ = ticket_handle
                                                .update_field("status", &next_status.to_string());
                                            needs_reload.set(true);
                                        }
                                    }
                                }
                            }
                        }
                        // Move ticket left (previous status)
                        KeyCode::Char('S') => {
                            let col = current_column.get();
                            let row = current_row.get();
                            if col > 0 {
                                let tickets_read = all_tickets.read();
                                let query = search_query.to_string();
                                if let Some(ticket) = get_ticket_at(&tickets_read, &query, col, row)
                                {
                                    drop(tickets_read);
                                    if let Some(id) = &ticket.id {
                                        let prev_status = COLUMNS[col - 1];
                                        if let Ok(ticket_handle) = Ticket::find(id) {
                                            let _ = ticket_handle
                                                .update_field("status", &prev_status.to_string());
                                            needs_reload.set(true);
                                        }
                                    }
                                }
                            }
                        }
                        // Toggle column visibility
                        KeyCode::Char('1') => {
                            let mut vis = visible_columns.get();
                            vis[0] = !vis[0];
                            visible_columns.set(vis);
                            adjust_column_after_toggle(&mut current_column, &vis);
                        }
                        KeyCode::Char('2') => {
                            let mut vis = visible_columns.get();
                            vis[1] = !vis[1];
                            visible_columns.set(vis);
                            adjust_column_after_toggle(&mut current_column, &vis);
                        }
                        KeyCode::Char('3') => {
                            let mut vis = visible_columns.get();
                            vis[2] = !vis[2];
                            visible_columns.set(vis);
                            adjust_column_after_toggle(&mut current_column, &vis);
                        }
                        KeyCode::Char('4') => {
                            let mut vis = visible_columns.get();
                            vis[3] = !vis[3];
                            visible_columns.set(vis);
                            adjust_column_after_toggle(&mut current_column, &vis);
                        }
                        KeyCode::Char('5') => {
                            let mut vis = visible_columns.get();
                            vis[4] = !vis[4];
                            visible_columns.set(vis);
                            adjust_column_after_toggle(&mut current_column, &vis);
                        }
                        // Edit selected ticket
                        KeyCode::Char('e') | KeyCode::Enter => {
                            let col = current_column.get();
                            let row = current_row.get();
                            let tickets_read = all_tickets.read();
                            let query = search_query.to_string();
                            if let Some(ticket) = get_ticket_at(&tickets_read, &query, col, row) {
                                drop(tickets_read);
                                if let Some(id) = &ticket.id
                                    && let Ok(ticket_handle) = Ticket::find(id)
                                {
                                    let body = ticket_handle
                                        .read_content()
                                        .ok()
                                        .map(|c| extract_body_for_edit(&c))
                                        .unwrap_or_default();
                                    editing_ticket.set(ticket);
                                    editing_body.set(body);
                                    is_editing_existing.set(true);
                                }
                            }
                        }
                        // Create new ticket
                        KeyCode::Char('n') => {
                            is_creating_new.set(true);
                            is_editing_existing.set(false);
                            editing_ticket.set(TicketMetadata::default());
                            editing_body.set(String::new());
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    });

    // Exit if requested
    if should_exit.get() {
        system.exit();
    }

    // Build column toggle indicator for header
    let column_toggles: String = visible_columns
        .get()
        .iter()
        .enumerate()
        .map(|(i, &visible)| {
            if visible {
                format!("[{}]", COLUMN_KEYS[i])
            } else {
                "[ ]".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("");

    let total_tickets = filtered.len();
    let tickets_ref_for_count = all_tickets.read();
    let all_ticket_count = tickets_ref_for_count.len();
    drop(tickets_ref_for_count);

    let theme = theme();

    // Get editing state for rendering
    let edit_ticket_ref = editing_ticket.read();
    let edit_ticket: Option<TicketMetadata> = if is_editing_existing.get() {
        Some(edit_ticket_ref.clone())
    } else {
        None
    };
    drop(edit_ticket_ref);
    let edit_body: Option<String> = if is_editing {
        Some(editing_body.to_string())
    } else {
        None
    };

    // Determine if we should show an empty state
    let empty_state_kind: Option<EmptyStateKind> = match init_result.get() {
        InitResult::NoJanusDir => Some(EmptyStateKind::NoJanusDir),
        InitResult::EmptyDir => {
            if all_ticket_count == 0 {
                Some(EmptyStateKind::NoTickets)
            } else {
                None
            }
        }
        InitResult::Ok => {
            if all_ticket_count == 0 {
                Some(EmptyStateKind::NoTickets)
            } else if total_tickets == 0 && !query_str.is_empty() {
                Some(EmptyStateKind::NoSearchResults)
            } else {
                None
            }
        }
    };

    // Show empty state if needed (except for no search results, which shows inline)
    let show_full_empty_state = matches!(
        empty_state_kind,
        Some(EmptyStateKind::NoJanusDir) | Some(EmptyStateKind::NoTickets)
    );

    // Determine shortcuts to show
    let shortcuts = if is_editing {
        edit_shortcuts()
    } else if show_full_empty_state {
        empty_shortcuts()
    } else {
        board_shortcuts()
    };

    // Calculate column width
    let visible_count = visible_indices.len().max(1) as u16;
    let column_width = width / visible_count;

    element! {
        View(
            width,
            height,
            flex_direction: FlexDirection::Column,
            background_color: theme.background,
        ) {
            // Header with column toggles
            View(
                width: 100pct,
                height: 1,
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                padding_left: 1,
                padding_right: 1,
                background_color: theme.highlight,
            ) {
                Text(
                    content: "Janus - Board",
                    color: theme.text,
                    weight: Weight::Bold,
                )
                View(flex_direction: FlexDirection::Row, gap: 1) {
                    Text(
                        content: format!("{} tickets", total_tickets),
                        color: theme.text_dimmed,
                    )
                    #(if !show_full_empty_state {
                        Some(element! {
                            Text(
                                content: column_toggles.clone(),
                                color: theme.text,
                            )
                        })
                    } else {
                        None
                    })
                }
            }

            #(if show_full_empty_state {
                // Show full-screen empty state
                Some(element! {
                    View(flex_grow: 1.0, width: 100pct) {
                        EmptyState(
                            kind: empty_state_kind.unwrap_or_default(),
                        )
                    }
                })
            } else {
                // Normal board view
                Some(element! {
                    View(
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Column,
                        width: 100pct,
                    ) {
                        // Search bar
                        View(
                            width: 100pct,
                            height: 1,
                            padding_left: 1,
                            padding_right: 1,
                            margin_top: 1,
                        ) {
                            InlineSearchBox(
                                value: Some(search_query),
                                has_focus: search_focused.get() && !is_editing,
                            )
                        }

                        // Show empty state for no search results
                        #(if empty_state_kind == Some(EmptyStateKind::NoSearchResults) {
                            Some(element! {
                                View(
                                    flex_grow: 1.0,
                                    width: 100pct,
                                ) {
                                    EmptyState(
                                        kind: EmptyStateKind::NoSearchResults,
                                        search_query: Some(query_str.clone()),
                                    )
                                }
                            })
                        } else {
                            Some(element! {
                                View(
                                    flex_grow: 1.0,
                                    flex_direction: FlexDirection::Column,
                                    width: 100pct,
                                ) {
                                    // Column headers
                                    View(
                                        width: 100pct,
                                        height: 2,
                                        flex_direction: FlexDirection::Row,
                                        margin_top: 1,
                                    ) {
                                        #(visible_indices.iter().map(|&col_idx| {
                                            let status = COLUMNS[col_idx];
                                            let name = COLUMN_NAMES[col_idx];
                                            let count = tickets_by_status.get(col_idx).map(|v| v.len()).unwrap_or(0);
                                            let is_active = current_column.get() == col_idx && !search_focused.get();
                                            let status_color = theme.status_color(status);

                                            element! {
                                                View(
                                                    width: column_width,
                                                    flex_direction: FlexDirection::Column,
                                                    align_items: AlignItems::Center,
                                                    border_edges: Edges::Bottom,
                                                    border_style: BorderStyle::Single,
                                                    border_color: if is_active { theme.border_focused } else { theme.border },
                                                ) {
                                                    Text(
                                                        content: name,
                                                        color: if is_active { status_color } else { theme.text_dimmed },
                                                        weight: if is_active { Weight::Bold } else { Weight::Normal },
                                                    )
                                                    Text(
                                                        content: count.to_string(),
                                                        color: theme.text_dimmed,
                                                    )
                                                }
                                            }
                                        }))
                                    }

                                    // Column content
                                    View(
                                        flex_grow: 1.0,
                                        width: 100pct,
                                        flex_direction: FlexDirection::Row,
                                        overflow: Overflow::Hidden,
                                    ) {
                                        #(visible_indices.iter().map(|&col_idx| {
                                            let column_tickets = tickets_by_status.get(col_idx).cloned().unwrap_or_default();
                                            let is_active_column = current_column.get() == col_idx && !search_focused.get();
                                            let current_row_val = current_row.get();

                                            element! {
                                                View(
                                                    width: column_width,
                                                    height: 100pct,
                                                    flex_direction: FlexDirection::Column,
                                                    padding: 1,
                                                    gap: 1,
                                                    border_edges: Edges::Right,
                                                    border_style: BorderStyle::Single,
                                                    border_color: theme.border,
                                                    overflow: Overflow::Hidden,
                                                ) {
                                                    #(column_tickets.iter().enumerate().take(available_height as usize / 4).map(|(row_idx, ft)| {
                                                        let is_selected = is_active_column && row_idx == current_row_val;
                                                        element! {
                                                            TicketCard(
                                                                ticket: ft.ticket.clone(),
                                                                is_selected: is_selected,
                                                            )
                                                        }
                                                    }))
                                                }
                                            }
                                        }))
                                    }
                                }
                            })
                        })
                    }
                })
            })

            // Footer
            Footer(shortcuts: shortcuts)

            // Edit form overlay
            #(if is_editing {
                Some(element! {
                    EditForm(
                        ticket: edit_ticket.clone(),
                        initial_body: edit_body.clone(),
                        on_close: Some(edit_result),
                    )
                })
            } else {
                None
            })
        }
    }
}

/// Adjust current column to first visible column if current is hidden
fn adjust_column_after_toggle(current_column: &mut State<usize>, visible: &[bool; 5]) {
    let current = current_column.get();
    if !visible[current] {
        // Find first visible column
        if let Some(first_visible) = visible.iter().position(|&v| v) {
            current_column.set(first_visible);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_columns_constant() {
        assert_eq!(COLUMNS.len(), 5);
        assert_eq!(COLUMNS[0], TicketStatus::New);
        assert_eq!(COLUMNS[4], TicketStatus::Cancelled);
    }

    #[test]
    fn test_column_names() {
        assert_eq!(COLUMN_NAMES.len(), 5);
        assert_eq!(COLUMN_NAMES[0], "NEW");
    }

    #[test]
    fn test_get_column_tickets() {
        use crate::types::{TicketPriority, TicketType};

        let tickets = vec![
            FilteredTicket {
                ticket: TicketMetadata {
                    id: Some("j-a1b2".to_string()),
                    status: Some(TicketStatus::New),
                    priority: Some(TicketPriority::P2),
                    ticket_type: Some(TicketType::Task),
                    ..Default::default()
                },
                score: 0,
                title_indices: vec![],
            },
            FilteredTicket {
                ticket: TicketMetadata {
                    id: Some("j-c3d4".to_string()),
                    status: Some(TicketStatus::InProgress),
                    priority: Some(TicketPriority::P1),
                    ticket_type: Some(TicketType::Bug),
                    ..Default::default()
                },
                score: 0,
                title_indices: vec![],
            },
        ];

        let new_tickets = get_column_tickets(&tickets, TicketStatus::New);
        assert_eq!(new_tickets.len(), 1);
        assert_eq!(new_tickets[0].ticket.id, Some("j-a1b2".to_string()));

        let wip_tickets = get_column_tickets(&tickets, TicketStatus::InProgress);
        assert_eq!(wip_tickets.len(), 1);
        assert_eq!(wip_tickets[0].ticket.id, Some("j-c3d4".to_string()));
    }
}
