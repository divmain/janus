//! Kanban board view (`janus board`)
//!
//! Provides an interactive TUI for viewing and managing tickets organized
//! by status in a kanban-style board layout with columns for each status.

pub mod handlers;

use iocraft::prelude::*;

use crate::tui::components::{
    EmptyState, EmptyStateKind, Footer, InlineSearchBox, TicketCard, board_shortcuts,
    edit_shortcuts, empty_shortcuts,
};
use crate::tui::edit::{EditForm, EditResult};
use crate::tui::edit_state::EditFormState;
use crate::tui::search::{FilteredTicket, filter_tickets};
use crate::tui::state::{InitResult, TuiState};
use crate::tui::theme::theme;
use crate::types::{TicketMetadata, TicketStatus};

use handlers::BoardHandlerContext;

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
    let init_result: State<InitResult> = hooks.use_state(|| TuiState::init_sync().1);
    let mut all_tickets: State<Vec<TicketMetadata>> =
        hooks.use_state(|| TuiState::new_sync().repository.tickets);
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
        all_tickets.set(TuiState::new_sync().repository.tickets);
    }

    // Handle edit form result using shared EditFormState
    {
        let mut edit_state = EditFormState {
            result: &mut edit_result,
            is_editing_existing: &mut is_editing_existing,
            is_creating_new: &mut is_creating_new,
            editing_ticket: &mut editing_ticket,
            editing_body: &mut editing_body,
        };
        if edit_state.handle_result() {
            needs_reload.set(true);
        }
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
                    // Create handler context
                    let mut ctx = BoardHandlerContext {
                        search_query: &mut search_query,
                        search_focused: &mut search_focused,
                        should_exit: &mut should_exit,
                        needs_reload: &mut needs_reload,
                        visible_columns: &mut visible_columns,
                        current_column: &mut current_column,
                        current_row: &mut current_row,
                        edit_result: &mut edit_result,
                        is_editing_existing: &mut is_editing_existing,
                        is_creating_new: &mut is_creating_new,
                        editing_ticket: &mut editing_ticket,
                        editing_body: &mut editing_body,
                        all_tickets: &all_tickets,
                    };

                    handlers::handle_key_event(&mut ctx, code, modifiers);
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

    // Get editing state for rendering using shared EditFormState
    let (edit_ticket, edit_body) = {
        let edit_state = EditFormState {
            result: &mut edit_result,
            is_editing_existing: &mut is_editing_existing,
            is_creating_new: &mut is_creating_new,
            editing_ticket: &mut editing_ticket,
            editing_body: &mut editing_body,
        };
        (edit_state.get_edit_ticket(), edit_state.get_edit_body())
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
