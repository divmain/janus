//! Kanban board view (`janus board`)
//!
//! Provides an interactive TUI for viewing and managing tickets organized
//! by status in a kanban-style board layout with columns for each status.

pub mod handlers;
pub mod model;

use iocraft::prelude::*;

use crate::ticket::Ticket;
use crate::tui::components::{
    Clickable, ClickableText, EmptyState, EmptyStateKind, InlineSearchBox, TicketCard, Toast,
    board_shortcuts, compute_empty_state, edit_shortcuts, empty_shortcuts,
};
use crate::tui::edit::{EditFormOverlay, EditResult};
use crate::tui::edit_state::{EditFormState, EditMode};
use crate::tui::hooks::use_ticket_loader;
use crate::tui::repository::InitResult;
use crate::tui::screen_base::{ScreenLayout, should_process_key_event};
use crate::tui::search::FilteredTicket;
use crate::tui::search_orchestrator::{SearchState, compute_filtered_tickets};
use crate::tui::theme::theme;
use crate::types::{TicketMetadata, TicketStatus};

use handlers::{BoardAsyncHandlers, BoardHandlerContext};
use model::{COLUMN_KEYS, COLUMN_NAMES, COLUMNS};

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

    // State management - initialize with empty state, load asynchronously
    let init_result: State<InitResult> = hooks.use_state(|| InitResult::Ok);
    let all_tickets: State<Vec<TicketMetadata>> = hooks.use_state(Vec::new);
    let mut is_loading = hooks.use_state(|| true);
    let toast: State<Option<Toast>> = hooks.use_state(|| None);
    let mut search_query = hooks.use_state(String::new);
    let mut should_exit = hooks.use_state(|| false);
    let mut needs_reload = hooks.use_state(|| false);

    // Search state - search is executed on Enter, not while typing
    let mut search_state = SearchState::use_state(&mut hooks);

    // Edit form state - single enum tracks the editing mode
    let mut edit_mode: State<EditMode> = hooks.use_state(EditMode::default);
    let mut edit_result: State<EditResult> = hooks.use_state(EditResult::default);

    // Async load handler with minimum 100ms display time to prevent UI flicker
    // NOTE: This must be created before update_status_handler so it can be cloned into it
    let load_handler: Handler<()> =
        hooks.use_async_handler(use_ticket_loader(all_tickets, is_loading, init_result));

    // Direct async handler for update status operations (replaces action queue pattern)
    let update_status_handler: Handler<(String, TicketStatus)> = hooks.use_async_handler({
        let toast_setter = toast;
        let all_tickets_setter = all_tickets;
        move |(ticket_id, status): (String, TicketStatus)| {
            let mut toast_setter = toast_setter;
            let mut all_tickets_setter = all_tickets_setter;
            async move {
                match Ticket::find(&ticket_id).await {
                    Ok(ticket) => match ticket.update_field("status", &status.to_string()) {
                        Ok(_) => {
                            toast_setter.set(Some(Toast::success(format!(
                                "Updated {ticket_id} to {status}"
                            ))));
                            // Refresh the mutated ticket in the store, then update in-place
                            crate::tui::repository::TicketRepository::refresh_ticket_in_store(
                                &ticket_id,
                            )
                            .await;
                            let current = all_tickets_setter.read().clone();
                            let tickets =
                                crate::tui::repository::TicketRepository::refresh_single_ticket(
                                    current, &ticket_id,
                                )
                                .await;
                            all_tickets_setter.set(tickets);
                        }
                        Err(e) => {
                            toast_setter.set(Some(Toast::error(format!("Failed to update: {e}"))));
                        }
                    },
                    Err(e) => {
                        toast_setter.set(Some(Toast::error(format!("Ticket not found: {e}"))));
                    }
                }
            }
        }
    });

    // Trigger initial load on mount
    let mut load_started = hooks.use_state(|| false);
    if !load_started.get() {
        load_started.set(true);
        load_handler.clone()(());
    }

    // Subscribe to store watcher events for live external updates.
    // The watcher updates the in-memory store when external processes modify ticket files.
    // This future polls the broadcast channel and sets needs_reload to refresh the UI.
    hooks.use_future({
        let mut needs_reload = needs_reload;
        async move {
            if let Some(mut rx) = crate::cache::subscribe_to_changes() {
                loop {
                    match rx.recv().await {
                        Ok(_event) => {
                            needs_reload.set(true);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            needs_reload.set(true);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            }
        }
    });

    // Column visibility state (all visible by default)
    let mut visible_columns = hooks.use_state(|| [true; 5]);

    // Navigation state
    let mut current_column = hooks.use_state(|| 0usize);
    let mut current_row = hooks.use_state(|| 0usize);
    let mut column_scroll_offsets = hooks.use_state(|| [0usize; 5]);
    let mut search_focused = hooks.use_state(|| false);

    // Reload tickets if needed - use async handler instead of sync
    if needs_reload.get() && !is_loading.get() {
        needs_reload.set(false);
        is_loading.set(true);
        load_handler.clone()(());
    }

    // Handle edit form result using shared EditFormState
    {
        let mut edit_state = EditFormState {
            mode: &mut edit_mode,
            result: &mut edit_result,
        };
        if edit_state.handle_result() {
            needs_reload.set(true);
        }
    }

    let is_editing = !matches!(*edit_mode.read(), EditMode::None);

    // Compute filtered tickets
    let query_str = search_query.to_string();

    search_state.check_pending(query_str.clone());
    search_state.clear_if_empty(&query_str);

    let filtered = compute_filtered_tickets(&all_tickets.read(), &search_state, &query_str);

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

    // Ensure at least one column is visible to prevent division by zero
    // If all columns are hidden, default to showing the first column
    let visible_count = visible_indices.len().max(1);

    // Calculate column width as equal percentage for all visible columns
    let column_width_pct = 100.0 / visible_count as f32;

    // Calculate card width in characters for multi-line title wrapping
    // Terminal width minus overall padding (2 chars), divided by number of columns,
    // then subtract column padding (2) and border (1)
    let column_char_width = (width as u32).saturating_sub(2) / visible_count as u32;
    let card_width = column_char_width.saturating_sub(5); // padding + borders

    // Calculate column heights
    // Layout overhead: header (1) + search bar (1) + search margin (1) +
    // column headers (2) + column header margin (1) + footer (1) = 7 lines
    let available_height = height.saturating_sub(7);
    // Each card can be up to 7 lines: border (2) + ID (1) + title (1-3) + priority (1) + margin (1)
    // Reserve 2 lines for "X more above/below" indicators
    // Use 6 as average card height estimate
    let cards_per_column = (available_height.saturating_sub(2) / 6).max(1) as usize;

    // Clone handler for use in event handler closure
    let update_status_handler_for_events = update_status_handler.clone();

    // Keyboard event handling
    hooks.use_terminal_events({
        move |event| {
            if is_editing {
                return;
            }

            match event {
                TerminalEvent::Key(KeyEvent {
                    code,
                    kind,
                    modifiers,
                    ..
                }) if should_process_key_event(kind) => {
                    let mut ctx = BoardHandlerContext {
                        search_query: &mut search_query,
                        search_focused: &mut search_focused,
                        search_orchestrator: &mut search_state,
                        should_exit: &mut should_exit,
                        needs_reload: &mut needs_reload,
                        visible_columns: &mut visible_columns,
                        current_column: &mut current_column,
                        current_row: &mut current_row,
                        column_scroll_offsets: &mut column_scroll_offsets,
                        column_height: cards_per_column,
                        edit_mode: &mut edit_mode,
                        edit_result: &mut edit_result,
                        all_tickets: &all_tickets,
                        handlers: BoardAsyncHandlers {
                            update_status: &update_status_handler_for_events,
                        },
                        cache: std::cell::RefCell::new(None),
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

    let total_tickets = filtered.len();
    let tickets_ref_for_count = all_tickets.read();
    let all_ticket_count = tickets_ref_for_count.len();
    drop(tickets_ref_for_count);

    let theme = theme();

    // Create column toggle handlers OUTSIDE the iterator to follow rules of hooks
    // Hooks must be called in the same order every render
    let column_toggle_handlers: Vec<Handler<()>> = (0..5)
        .map(|i| {
            hooks.use_async_handler({
                let cols = visible_columns;
                let cur_col = current_column;
                move |()| {
                    let mut cols = cols;
                    let mut cur_col = cur_col;
                    async move {
                        let mut vis = cols.get();
                        vis[i] = !vis[i];
                        cols.set(vis);
                        // Adjust current column if it was hidden
                        handlers::adjust_column_after_toggle(&mut cur_col, &vis);
                    }
                }
            })
        })
        .collect();

    // Build column toggle indicators using ClickableText components
    let visible_cols = visible_columns.get();
    let column_toggles_elements: Vec<AnyElement<'static>> = (0..5)
        .map(|i| {
            let is_visible = visible_cols[i];
            let key = COLUMN_KEYS[i];
            let on_click = column_toggle_handlers[i].clone();

            element! {
                ClickableText(
                    content: if is_visible {
                        format!("[{key}]")
                    } else {
                        "[ ]".to_string()
                    },
                    on_click: Some(on_click),
                    color: Some(if is_visible { theme.text } else { theme.text_dimmed }),
                    hover_color: Some(theme.border_focused),
                    weight: Some(Weight::Normal),
                    hover_weight: Some(Weight::Bold),
                )
            }
            .into()
        })
        .collect();

    // Get editing state for rendering using shared EditFormState
    let (edit_ticket, edit_body) = {
        let edit_state = EditFormState {
            mode: &mut edit_mode,
            result: &mut edit_result,
        };
        (edit_state.get_edit_ticket(), edit_state.get_edit_body())
    };

    // Determine if we should show an empty state
    let empty_state_kind = compute_empty_state(
        is_loading.get(),
        init_result.get(),
        all_ticket_count,
        total_tickets,
        &query_str,
    );

    // Show empty state if needed (except for no search results, which shows inline)
    let show_full_empty_state = matches!(
        empty_state_kind,
        Some(EmptyStateKind::NoJanusDir)
            | Some(EmptyStateKind::NoTickets)
            | Some(EmptyStateKind::Loading)
    );

    // Determine shortcuts to show
    let shortcuts = if is_editing {
        edit_shortcuts()
    } else if show_full_empty_state {
        empty_shortcuts()
    } else {
        board_shortcuts()
    };

    // Create scroll handlers for ALL 5 columns OUTSIDE the iterator
    // This follows the rules of hooks - hooks must be called in the same order every render
    let scroll_up_handlers: Vec<Handler<()>> = (0..5)
        .map(|col_idx| {
            hooks.use_async_handler({
                let col_scroll_offsets = column_scroll_offsets;
                move |()| {
                    let mut col_scroll_offsets = col_scroll_offsets;
                    async move {
                        let mut offsets = col_scroll_offsets.get();
                        offsets[col_idx] = offsets[col_idx].saturating_sub(3);
                        col_scroll_offsets.set(offsets);
                    }
                }
            })
        })
        .collect();

    let scroll_down_handlers: Vec<Handler<()>> = (0..5)
        .map(|col_idx| {
            hooks.use_async_handler({
                let col_scroll_offsets = column_scroll_offsets;
                let _cards_per_col = cards_per_column;
                move |()| {
                    let mut col_scroll_offsets = col_scroll_offsets;
                    async move {
                        let mut offsets = col_scroll_offsets.get();
                        // Get the total count for this column dynamically
                        // We use a large max value and let the view handle clamping
                        offsets[col_idx] += 3;
                        col_scroll_offsets.set(offsets);
                    }
                }
            })
        })
        .collect();

    let page_up_handlers: Vec<Handler<()>> = (0..5)
        .map(|col_idx| {
            hooks.use_async_handler({
                let col_scroll_offsets = column_scroll_offsets;
                let cards_per_col = cards_per_column;
                move |()| {
                    let mut col_scroll_offsets = col_scroll_offsets;
                    async move {
                        let mut offsets = col_scroll_offsets.get();
                        offsets[col_idx] = offsets[col_idx].saturating_sub(cards_per_col);
                        col_scroll_offsets.set(offsets);
                    }
                }
            })
        })
        .collect();

    let page_down_handlers: Vec<Handler<()>> = (0..5)
        .map(|col_idx| {
            hooks.use_async_handler({
                let col_scroll_offsets = column_scroll_offsets;
                let cards_per_col = cards_per_column;
                move |()| {
                    let mut col_scroll_offsets = col_scroll_offsets;
                    async move {
                        let mut offsets = col_scroll_offsets.get();
                        offsets[col_idx] += cards_per_col;
                        col_scroll_offsets.set(offsets);
                    }
                }
            })
        })
        .collect();

    // Create card click handlers for all 5 columns OUTSIDE the iterator
    // This follows the rules of hooks - hooks must be called in the same order every render
    // The handler accepts the row index as a parameter to select the correct ticket
    let card_click_handlers: Vec<Handler<usize>> = (0..5)
        .map(|col_idx| {
            hooks.use_async_handler({
                let cur_col = current_column;
                let cur_row = current_row;
                let search_focus = search_focused;
                move |row_idx: usize| {
                    let mut cur_col = cur_col;
                    let mut cur_row = cur_row;
                    let mut search_focus = search_focus;
                    async move {
                        cur_col.set(col_idx);
                        cur_row.set(row_idx);
                        search_focus.set(false);
                    }
                }
            })
        })
        .collect();

    element! {
        ScreenLayout(
            width: width,
            height: height,
            header_title: Some("Janus - Board"),
            header_ticket_count: Some(total_tickets),
            header_extra: Some(column_toggles_elements),
            shortcuts: shortcuts,
            toast: toast.read().clone(),
        ) {
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
                        overflow: Overflow::Hidden,
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
                                is_semantic: query_str.starts_with('~'),
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
                                                    width: Size::Percent(column_width_pct),
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

                                            // Get scroll offset for this column
                                            let scroll_offsets = column_scroll_offsets.get();
                                            let scroll_offset = scroll_offsets[col_idx];
                                            let total_count = column_tickets.len();

                                            // Calculate visible range
                                            let start = scroll_offset.min(total_count);
                                            let end = (scroll_offset + cards_per_column).min(total_count);

                                            // Calculate scroll indicators
                                            let hidden_above = start;
                                            let hidden_below = total_count.saturating_sub(end);

                                            // Use pre-created scroll handlers (created outside iterator to follow rules of hooks)
                                            let scroll_up_handler = scroll_up_handlers[col_idx].clone();
                                            let scroll_down_handler = scroll_down_handlers[col_idx].clone();
                                            let page_up_handler = page_up_handlers[col_idx].clone();
                                            let page_down_handler = page_down_handlers[col_idx].clone();

                                            element! {
                                                Clickable(
                                                    on_scroll_up: Some(scroll_up_handler),
                                                    on_scroll_down: Some(scroll_down_handler),
                                                ) {
                                                    View(
                                                        width: Size::Percent(column_width_pct),
                                                        height: 100pct,
                                                        flex_direction: FlexDirection::Column,
                                                        padding_left: 1,
                                                        padding_right: 1,
                                                        padding_top: 0,
                                                        padding_bottom: 0,
                                                        border_edges: Edges::Right,
                                                        border_style: BorderStyle::Single,
                                                        border_color: theme.border,
                                                        overflow: Overflow::Hidden,
                                                    ) {
                                                    // "More above" indicator - clickable for page navigation
                                                    #(if hidden_above > 0 {
                                                        Some(element! {
                                                            View(height: 1, padding_left: 1) {
                                                                ClickableText(
                                                                    content: format!("  {} more above", hidden_above),
                                                                    on_click: Some(page_up_handler),
                                                                    color: Some(theme.text_dimmed),
                                                                    hover_color: Some(theme.border_focused),
                                                                    weight: Some(Weight::Normal),
                                                                    hover_weight: Some(Weight::Bold),
                                                                )
                                                            }
                                                        })
                                                    } else {
                                                        None
                                                    })

                                                    // Visible cards (clickable)
                                                    // Use pre-created handler from card_click_handlers vector
                                                    // This follows the rules of hooks - hooks are called at top level, not in iterators
                                                    // row_idx is passed to handler to set the correct selected ticket
                                                    #(column_tickets.iter().enumerate().skip(start).take(end - start).map(|(row_idx, ft)| {
                                                        let is_selected = is_active_column && row_idx == current_row_val;
                                                        element! {
                                                            TicketCard(
                                                                ticket: ft.ticket.as_ref().clone(),
                                                                is_selected: is_selected,
                                                                width: Some(card_width),
                                                                on_click: Some(card_click_handlers[col_idx].clone()),
                                                                row_idx: row_idx,
                                                            )
                                                        }
                                                    }))

                                                    // Spacer to push "more below" to bottom
                                                    View(flex_grow: 1.0)

                                                    // "More below" indicator - clickable for page navigation
                                                    #(if hidden_below > 0 {
                                                        Some(element! {
                                                            View(height: 1, padding_left: 1) {
                                                                ClickableText(
                                                                    content: format!("  {} more below", hidden_below),
                                                                    on_click: Some(page_down_handler),
                                                                    color: Some(theme.text_dimmed),
                                                                    hover_color: Some(theme.border_focused),
                                                                    weight: Some(Weight::Normal),
                                                                    hover_weight: Some(Weight::Bold),
                                                                )
                                                            }
                                                        })
                                                    } else {
                                                        None
                                                    })
                                                }
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

            // Edit form overlay
            #(if is_editing {
                Some(element! {
                    EditFormOverlay(
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
    use crate::types::TicketId;
    use std::sync::Arc;

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
                ticket: Arc::new(TicketMetadata {
                    id: Some(TicketId::new_unchecked("j-a1b2")),
                    status: Some(TicketStatus::New),
                    priority: Some(TicketPriority::P2),
                    ticket_type: Some(TicketType::Task),
                    ..Default::default()
                }),
                score: 0,
                title_indices: vec![],
                is_semantic: false,
            },
            FilteredTicket {
                ticket: Arc::new(TicketMetadata {
                    id: Some(TicketId::new_unchecked("j-c3d4")),
                    status: Some(TicketStatus::InProgress),
                    priority: Some(TicketPriority::P1),
                    ticket_type: Some(TicketType::Bug),
                    ..Default::default()
                }),
                score: 0,
                title_indices: vec![],
                is_semantic: false,
            },
        ];

        let new_tickets = get_column_tickets(&tickets, TicketStatus::New);
        assert_eq!(new_tickets.len(), 1);
        assert_eq!(new_tickets[0].ticket.id.as_deref(), Some("j-a1b2"));

        let wip_tickets = get_column_tickets(&tickets, TicketStatus::InProgress);
        assert_eq!(wip_tickets.len(), 1);
        assert_eq!(wip_tickets[0].ticket.id.as_deref(), Some("j-c3d4"));
    }
}
