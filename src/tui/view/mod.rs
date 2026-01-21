//! Issue browser view (`janus view`)
//!
//! Provides an interactive TUI for browsing and managing tickets with
//! fuzzy search, keyboard navigation, and inline detail viewing.

pub mod handlers;
pub mod model;

use iocraft::prelude::*;
use tokio::sync::{Mutex, mpsc};

use crate::tui::components::{
    EmptyState, EmptyStateKind, Footer, Header, SearchBox, TicketDetail, TicketList, Toast,
    ToastNotification, browser_shortcuts, compute_empty_state, edit_shortcuts, empty_shortcuts,
    search_shortcuts,
};
use crate::tui::edit::{EditForm, EditResult};
use crate::tui::edit_state::EditFormState;
use crate::tui::hooks::use_ticket_loader;
use crate::tui::repository::InitResult;
use crate::tui::search::{FilteredTicket, filter_tickets};
use crate::tui::services::TicketService;
use crate::tui::state::Pane;
use crate::tui::theme::theme;
use crate::types::TicketMetadata;

use handlers::ViewAction;

/// Props for the IssueBrowser component
#[derive(Default, Props)]
pub struct IssueBrowserProps {}

/// Main issue browser component
///
/// Layout:
/// ```text
/// +------------------------------------------+
/// | Header                                    |
/// +------------------------------------------+
/// | SearchBox                                 |
/// +--------------------+---------------------+
/// | TicketList         | TicketDetail        |
/// |                    |                     |
/// |                    |                     |
/// +--------------------+---------------------+
/// | Footer                                    |
/// +------------------------------------------+
/// ```
#[component]
pub fn IssueBrowser<'a>(_props: &IssueBrowserProps, mut hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let (width, height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();

    // State management - initialize with empty state, load asynchronously
    let init_result: State<InitResult> = hooks.use_state(|| InitResult::Ok);
    let all_tickets: State<Vec<TicketMetadata>> = hooks.use_state(Vec::new);
    let mut is_loading = hooks.use_state(|| true);
    let toast: State<Option<Toast>> = hooks.use_state(|| None);
    let mut search_query = hooks.use_state(String::new);
    let mut selected_index = hooks.use_state(|| 0usize);
    let mut scroll_offset = hooks.use_state(|| 0usize);
    let mut detail_scroll_offset = hooks.use_state(|| 0usize);
    let mut active_pane = hooks.use_state(|| Pane::List); // Start on list, not search
    let mut should_exit = hooks.use_state(|| false);
    let mut needs_reload = hooks.use_state(|| false);

    // Async load handler with minimum 100ms display time to prevent UI flicker
    let load_handler: Handler<()> =
        hooks.use_async_handler(use_ticket_loader(all_tickets, is_loading, init_result));

    // Trigger initial load on mount
    let mut load_started = hooks.use_state(|| false);
    if !load_started.get() {
        load_started.set(true);
        load_handler.clone()(());
    }

    // Edit form state - use bool flags and separate storage for non-Copy data
    let mut edit_result: State<EditResult> = hooks.use_state(EditResult::default);
    let mut is_editing_existing = hooks.use_state(|| false);
    let mut is_creating_new = hooks.use_state(|| false);
    let mut editing_ticket_id = hooks.use_state(String::new);
    let mut editing_ticket: State<TicketMetadata> = hooks.use_state(TicketMetadata::default);
    let mut editing_body = hooks.use_state(String::new);

    // Action queue for async ticket operations
    // Channel is created once via use_state initializer - the tuple is split across two state slots
    // Note: We store the channel parts in a shared struct to ensure they're from the same channel
    struct ActionChannel {
        tx: mpsc::UnboundedSender<ViewAction>,
        rx: std::sync::Arc<Mutex<mpsc::UnboundedReceiver<ViewAction>>>,
    }
    let channel: State<ActionChannel> = hooks.use_state(|| {
        let (tx, rx) = mpsc::unbounded_channel::<ViewAction>();
        ActionChannel {
            tx,
            rx: std::sync::Arc::new(Mutex::new(rx)),
        }
    });
    let action_sender = channel.read().tx.clone();
    let action_channel = channel.read().rx.clone();

    // Async action queue processor
    // This handler processes pending actions from the queue
    let action_processor: Handler<()> = hooks.use_async_handler({
        let action_channel = action_channel.clone();
        let needs_reload_setter = needs_reload;
        let toast_setter = toast;
        let editing_ticket_id_setter = editing_ticket_id;
        let editing_ticket_setter = editing_ticket;
        let editing_body_setter = editing_body;
        let is_editing_setter = is_editing_existing;

        move |()| {
            let action_channel = action_channel.clone();
            let mut needs_reload_setter = needs_reload_setter;
            let mut toast_setter = toast_setter;
            let mut editing_ticket_id_setter = editing_ticket_id_setter;
            let mut editing_ticket_setter = editing_ticket_setter;
            let mut editing_body_setter = editing_body_setter;
            let mut is_editing_setter = is_editing_setter;

            async move {
                const MAX_BATCH: usize = 10;

                // Collect pending actions from the channel with bounded batch
                let actions: Vec<ViewAction> = {
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
                        ViewAction::CycleStatus { id } => {
                            match TicketService::cycle_status(&id).await {
                                Ok(_) => {
                                    should_reload = true;
                                }
                                Err(e) => {
                                    toast_setter.set(Some(Toast::error(format!(
                                        "Failed to cycle status: {}",
                                        e
                                    ))));
                                }
                            }
                        }
                        ViewAction::LoadForEdit { id } => {
                            match TicketService::load_for_edit(&id).await {
                                Ok((metadata, body)) => {
                                    editing_ticket_id_setter.set(id);
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
                            }
                        }
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
            editing_ticket_id.set(String::new());
            needs_reload.set(true);
        }
    }

    // Check if edit form is open
    let is_editing = is_editing_existing.get() || is_creating_new.get();

    // Compute filtered tickets
    let query_str = search_query.to_string();
    let tickets_ref = all_tickets.read();
    let filtered: Vec<FilteredTicket> = filter_tickets(&tickets_ref, &query_str);
    drop(tickets_ref); // Release the read lock
    let filtered_clone = filtered.clone();

    // Get the currently selected ticket
    let selected_ticket = filtered
        .get(selected_index.get())
        .map(|ft| ft.ticket.clone());

    // Calculate available height for the list
    // Total height - header (1) - search box (3) - footer (1) - borders (2)
    let list_height = height.saturating_sub(7) as usize;

    // Keyboard event handling
    hooks.use_terminal_events({
        let filtered_len = filtered_clone.len();
        let filtered_for_events = filtered_clone.clone();
        let action_sender_for_events = action_sender.clone();
        move |event| {
            // Skip if edit form is open (it handles its own events)
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
                    let mut ctx = handlers::ViewHandlerContext {
                        search_query: &mut search_query,
                        selected_index: &mut selected_index,
                        scroll_offset: &mut scroll_offset,
                        detail_scroll_offset: &mut detail_scroll_offset,
                        active_pane: &mut active_pane,
                        should_exit: &mut should_exit,
                        needs_reload: &mut needs_reload,
                        edit_result: &mut edit_result,
                        is_editing_existing: &mut is_editing_existing,
                        is_creating_new: &mut is_creating_new,
                        editing_ticket_id: &mut editing_ticket_id,
                        editing_ticket: &mut editing_ticket,
                        editing_body: &mut editing_body,
                        filtered_count: filtered_len,
                        list_height,
                        filtered_tickets: &filtered_for_events,
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

    // Reset selection if it's out of bounds after filtering
    if selected_index.get() >= filtered.len() && !filtered.is_empty() {
        selected_index.set(filtered.len() - 1);
    }
    if scroll_offset.get() > selected_index.get() {
        scroll_offset.set(selected_index.get());
    }

    let ticket_count = filtered.len();
    let tickets_ref_for_count = all_tickets.read();
    let total_ticket_count = tickets_ref_for_count.len();
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
        total_ticket_count,
        ticket_count,
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
        match active_pane.get() {
            Pane::Search => search_shortcuts(),
            _ => browser_shortcuts(),
        }
    };

    element! {
        View(
            width,
            height,
            flex_direction: FlexDirection::Column,
            background_color: theme.background,
        ) {
            // Header
            Header(
                subtitle: Some("Browser"),
                ticket_count: Some(ticket_count),
            )

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
                // Normal view with search box and content
                Some(element! {
                    View(
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Column,
                        width: 100pct,
                    ) {
                        // Search box
                        View(
                            width: 100pct,
                            padding_left: 1,
                            padding_right: 1,
                        ) {
                            SearchBox(
                                value: Some(search_query),
                                has_focus: active_pane.get() == Pane::Search && !is_editing,
                            )
                        }

                        // Main content area: List + Detail (or empty state for no results)
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
                                    flex_direction: FlexDirection::Row,
                                    width: 100pct,
                                ) {
                                    // Left pane: Ticket list (fixed width)
                                    View(
                                        width: 35pct,
                                        min_width: 35pct,
                                        max_width: 35pct,
                                        height: 100pct,
                                        flex_shrink: 0.0,
                                    ) {
                                        TicketList(
                                            tickets: filtered.clone(),
                                            selected_index: selected_index.get(),
                                            scroll_offset: scroll_offset.get(),
                                            has_focus: active_pane.get() == Pane::List && !is_editing,
                                            visible_height: list_height,
                                        )
                                    }

                                    // Right pane: Ticket detail (takes remaining space)
                                    View(
                                        width: 65pct,
                                        min_width: 65pct,
                                        max_width: 65pct,
                                        height: 100pct,
                                        flex_shrink: 0.0,
                                    ) {
                                        TicketDetail(
                                            ticket: selected_ticket.clone(),
                                            has_focus: active_pane.get() == Pane::Detail && !is_editing,
                                            scroll_offset: detail_scroll_offset.get(),
                                            visible_height: list_height,
                                        )
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
