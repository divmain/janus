//! Kanban board view (`janus board`)
//!
//! Provides an interactive TUI for viewing and managing tickets organized
//! by status in a kanban-style board layout with columns for each status.

pub mod handlers;
pub mod model;

use iocraft::prelude::*;
use tokio::sync::{Mutex, mpsc};

use crate::ticket::Ticket;
use crate::tui::components::{
    EmptyState, EmptyStateKind, Footer, InlineSearchBox, TicketCard, Toast, ToastNotification,
    board_shortcuts, compute_empty_state, edit_shortcuts, empty_shortcuts,
};
use crate::tui::edit::{EditFormOverlay, EditResult, extract_body_for_edit};
use crate::tui::edit_state::EditFormState;
use crate::tui::hooks::use_ticket_loader;
use crate::tui::repository::InitResult;
use crate::tui::search::{FilteredTicket, compute_title_highlights};
use crate::tui::theme::theme;
use crate::types::{TicketMetadata, TicketStatus};

use handlers::{BoardHandlerContext, TicketAction};

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

    // State management - initialize with empty state, load asynchronously
    let init_result: State<InitResult> = hooks.use_state(|| InitResult::Ok);
    let all_tickets: State<Vec<TicketMetadata>> = hooks.use_state(Vec::new);
    let mut is_loading = hooks.use_state(|| true);
    let toast: State<Option<Toast>> = hooks.use_state(|| None);
    let mut search_query = hooks.use_state(String::new);
    let mut should_exit = hooks.use_state(|| false);
    let mut needs_reload = hooks.use_state(|| false);

    // Search state - search is executed on Enter, not while typing
    // Store filtered tickets from search (Vec<FilteredTicket> with highlights)
    let mut search_filtered_tickets: State<Option<Vec<FilteredTicket>>> = hooks.use_state(|| None);
    // Track if search is currently running (for loading indicator)
    let mut search_in_flight = hooks.use_state(|| false);

    // Edit form state - declared early for use in action processor
    let mut edit_result: State<EditResult> = hooks.use_state(EditResult::default);
    let mut is_editing_existing = hooks.use_state(|| false);
    let mut is_creating_new = hooks.use_state(|| false);
    let mut editing_ticket: State<TicketMetadata> = hooks.use_state(TicketMetadata::default);
    let mut editing_body = hooks.use_state(String::new);

    // Action queue for async ticket operations
    // Channel is created once via use_state initializer - the tuple is split across two state slots
    // Note: We store the channel parts in a shared struct to ensure they're from the same channel
    struct ActionChannel {
        tx: mpsc::UnboundedSender<TicketAction>,
        rx: std::sync::Arc<Mutex<mpsc::UnboundedReceiver<TicketAction>>>,
    }
    let channel: State<ActionChannel> = hooks.use_state(|| {
        let (tx, rx) = mpsc::unbounded_channel::<TicketAction>();
        ActionChannel {
            tx,
            rx: std::sync::Arc::new(Mutex::new(rx)),
        }
    });
    let action_sender = channel.read().tx.clone();
    let action_channel = channel.read().rx.clone();

    // Async load handler with minimum 100ms display time to prevent UI flicker
    let load_handler: Handler<()> =
        hooks.use_async_handler(use_ticket_loader(all_tickets, is_loading, init_result));

    // Trigger initial load on mount
    let mut load_started = hooks.use_state(|| false);
    if !load_started.get() {
        load_started.set(true);
        load_handler.clone()(());
    }

    // Async search handler - executes SQL search via cache
    // Called when user presses Enter in search box
    let search_handler: Handler<String> = hooks.use_async_handler({
        let search_filtered_setter = search_filtered_tickets;
        let search_in_flight_setter = search_in_flight;

        move |query: String| {
            let mut search_filtered_setter = search_filtered_setter;
            let mut search_in_flight_setter = search_in_flight_setter;

            Box::pin(async move {
                if query.is_empty() {
                    // Empty query - clear results
                    search_filtered_setter.set(None);
                    search_in_flight_setter.set(false);
                    return;
                }

                // Execute SQL search via cache
                let results = if let Some(cache) = crate::cache::get_or_init_cache().await {
                    cache.search_tickets(&query).await.unwrap_or_default()
                } else {
                    vec![]
                };

                // Convert to FilteredTicket with title highlights
                let highlighted = compute_title_highlights(&results, &query);
                search_filtered_setter.set(Some(highlighted));
                search_in_flight_setter.set(false);
            })
        }
    });

    // Track if search needs to be triggered (set by Enter key handler)
    let mut pending_search = hooks.use_state(|| false);

    // Async action queue processor
    // This handler processes pending actions from the queue
    let action_processor: Handler<()> = hooks.use_async_handler({
        let action_channel = action_channel.clone();
        let needs_reload_setter = needs_reload;
        let toast_setter = toast;
        let editing_ticket_setter = editing_ticket;
        let editing_body_setter = editing_body;
        let is_editing_setter = is_editing_existing;

        move |()| {
            let action_channel = action_channel.clone();
            let mut needs_reload_setter = needs_reload_setter;
            let mut toast_setter = toast_setter;
            let mut editing_ticket_setter = editing_ticket_setter;
            let mut editing_body_setter = editing_body_setter;
            let mut is_editing_setter = is_editing_setter;

            async move {
                const MAX_BATCH: usize = 10;

                // Collect pending actions from the channel with bounded batch
                let actions: Vec<TicketAction> = {
                    let mut guard = action_channel.lock().await;
                    let mut actions = Vec::new();
                    while actions.len() < MAX_BATCH {
                        if let Ok(action) = guard.try_recv() {
                            actions.push(action);
                        } else {
                            break;
                        }
                    }
                    actions
                };

                if actions.is_empty() {
                    return;
                }

                let mut should_reload = false;

                // Process all pending actions sequentially
                for action in actions {
                    match action {
                        TicketAction::UpdateStatus { id, status } => {
                            match Ticket::find(&id).await {
                                Ok(ticket) => {
                                    if let Err(e) =
                                        ticket.update_field("status", &status.to_string())
                                    {
                                        toast_setter.set(Some(Toast::error(format!(
                                            "Failed to update {}: {}",
                                            id, e
                                        ))));
                                    } else {
                                        should_reload = true;
                                    }
                                }
                                Err(e) => {
                                    toast_setter.set(Some(Toast::error(format!(
                                        "Ticket not found: {}",
                                        e
                                    ))));
                                }
                            }
                        }
                        TicketAction::LoadForEdit { id } => match Ticket::find(&id).await {
                            Ok(ticket) => {
                                let metadata = ticket.read().unwrap_or_default();
                                let body = ticket
                                    .read_content()
                                    .ok()
                                    .map(|c| extract_body_for_edit(&c))
                                    .unwrap_or_default();
                                editing_ticket_setter.set(metadata);
                                editing_body_setter.set(body);
                                is_editing_setter.set(true);
                            }
                            Err(e) => {
                                toast_setter.set(Some(Toast::error(format!(
                                    "Failed to load ticket: {}",
                                    e
                                ))));
                            }
                        },
                    }
                }

                // Trigger reload if any status updates were made
                if should_reload {
                    needs_reload_setter.set(true);
                }
            }
        }
    });

    // Track if actions are pending (set when handlers send actions)
    let mut actions_pending = hooks.use_state(|| false);

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

    // Trigger search if pending (set by Enter key in search mode)
    if pending_search.get() {
        pending_search.set(false);
        search_in_flight.set(true);
        search_handler.clone()(query_str.clone());
    }

    // Clear search results if search box is empty
    if query_str.is_empty() && search_filtered_tickets.read().is_some() {
        search_filtered_tickets.set(None);
    }

    // Determine which tickets to display
    // - Empty query: show all tickets
    // - Have search results: show them (regardless of what's currently in search box)
    // - No search results and non-empty query: show all tickets (waiting for first search)
    let filtered: Vec<FilteredTicket> = if query_str.is_empty() {
        // Empty query - show all tickets
        let tickets_ref = all_tickets.read();
        let result = tickets_ref
            .iter()
            .map(|t| FilteredTicket {
                ticket: t.clone(),
                score: 0,
                title_indices: vec![],
            })
            .collect();
        drop(tickets_ref);
        result
    } else if let Some(results) = search_filtered_tickets.read().as_ref() {
        // Have search results - keep showing them until a new search is executed
        results.clone()
    } else {
        // No search results yet and query is non-empty - show all tickets
        let tickets_ref = all_tickets.read();
        let result = tickets_ref
            .iter()
            .map(|t| FilteredTicket {
                ticket: t.clone(),
                score: 0,
                title_indices: vec![],
            })
            .collect();
        drop(tickets_ref);
        result
    };

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
    let cards_per_column = (available_height.saturating_sub(2) / 4).max(1) as usize; // Each card is ~3-4 lines, reserve 2 for indicators

    // Keyboard event handling
    hooks.use_terminal_events({
        let action_sender_for_events = action_sender.clone();
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
                        pending_search: &mut pending_search,
                        should_exit: &mut should_exit,
                        needs_reload: &mut needs_reload,
                        visible_columns: &mut visible_columns,
                        current_column: &mut current_column,
                        current_row: &mut current_row,
                        column_scroll_offsets: &mut column_scroll_offsets,
                        column_height: cards_per_column,
                        edit_result: &mut edit_result,
                        is_editing_existing: &mut is_editing_existing,
                        is_creating_new: &mut is_creating_new,
                        editing_ticket: &mut editing_ticket,
                        editing_body: &mut editing_body,
                        all_tickets: &all_tickets,
                        action_tx: &action_sender_for_events,
                    };

                    handlers::handle_key_event(&mut ctx, code, modifiers);

                    // Trigger action processor if actions were queued
                    // The processor will handle any pending actions asynchronously
                    actions_pending.set(true);
                }
                _ => {}
            }
        }
    });

    // Process any pending actions from the queue
    if actions_pending.get() {
        actions_pending.set(false);
        action_processor(());
    }

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

    element! {
        View(
            width,
            height,
            flex_direction: FlexDirection::Column,
            background_color: theme.background,
            position: Position::Relative,
        ) {
            // Header with column toggles
            View(
                width: 100pct,
                height: 1,
                flex_direction: FlexDirection::Row,
                flex_shrink: 0.0,
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
                        content: if search_in_flight.get() {
                            "Searching...".to_string()
                        } else {
                            format!("{} tickets", total_tickets)
                        },
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
                                                    flex_grow: 1.0,
                                                    flex_shrink: 0.0,
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

                                            element! {
                                                View(
                                                    flex_grow: 1.0,
                                                    flex_shrink: 0.0,
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
                                                    // "More above" indicator
                                                    #(if hidden_above > 0 {
                                                        Some(element! {
                                                            View(height: 1, padding_left: 1) {
                                                                Text(
                                                                    content: format!("  {} more above", hidden_above),
                                                                    color: theme.text_dimmed,
                                                                )
                                                            }
                                                        })
                                                    } else {
                                                        None
                                                    })

                                                    // Visible cards
                                                    #(column_tickets.iter().enumerate().skip(start).take(end - start).map(|(row_idx, ft)| {
                                                        let is_selected = is_active_column && row_idx == current_row_val;
                                                        element! {
                                                            View(margin_top: 1) {
                                                                TicketCard(
                                                                    ticket: ft.ticket.clone(),
                                                                    is_selected: is_selected,
                                                                )
                                                            }
                                                        }
                                                    }))

                                                    // Spacer to push "more below" to bottom
                                                    View(flex_grow: 1.0)

                                                    // "More below" indicator
                                                    #(if hidden_below > 0 {
                                                        Some(element! {
                                                            View(height: 1, padding_left: 1) {
                                                                Text(
                                                                    content: format!("  {} more below", hidden_below),
                                                                    color: theme.text_dimmed,
                                                                )
                                                            }
                                                        })
                                                    } else {
                                                        None
                                                    })
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

            // Toast notification
            #({
                let toast_val = toast.read().clone();
                if toast_val.is_some() {
                    Some(element! {
                        ToastNotification(toast: toast_val)
                    })
                } else {
                    None
                }
            })

            // Footer
            Footer(shortcuts: shortcuts)

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
