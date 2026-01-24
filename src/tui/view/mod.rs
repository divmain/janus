//! Issue browser view (`janus view`)
//!
//! Provides an interactive TUI for browsing and managing tickets with
//! fuzzy search, keyboard navigation, and inline detail viewing.

pub mod handlers;
pub mod modals;
pub mod model;

use iocraft::prelude::*;
use std::pin::Pin;
use tokio::sync::{Mutex, mpsc};

use crate::formatting::extract_ticket_body;
use crate::ticket::Ticket;
use crate::tui::components::{
    EmptyState, EmptyStateKind, Footer, Header, SearchBox, TicketDetail, TicketList, Toast,
    ToastNotification, browser_shortcuts, cancel_confirm_modal_shortcuts, compute_empty_state,
    edit_shortcuts, empty_shortcuts, note_input_modal_shortcuts, search_shortcuts,
    triage_shortcuts,
};
use crate::tui::edit::{EditFormOverlay, EditResult};
use crate::tui::edit_state::EditFormState;
use crate::tui::hooks::use_ticket_loader;
use crate::tui::repository::InitResult;
use crate::tui::search::{FilteredTicket, compute_title_highlights};
use crate::tui::services::TicketService;
use crate::tui::state::Pane;
use crate::tui::theme::theme;
use crate::tui::action_queue::{Action, ActionResult, ActionQueueBuilder};
use crate::types::{TicketMetadata, TicketStatus};

use handlers::ViewAction;
use modals::{CancelConfirmModal, NoteInputModal};

/// Actions for the issue browser
#[derive(Debug, Clone)]
pub enum BrowserAction {
    CycleStatus { id: String },
    LoadForEdit { id: String },
    MarkTriaged { id: String, triaged: bool },
    CancelTicket { id: String },
    AddNote { id: String, note: String },
}

impl Action for BrowserAction {
    fn execute(self) -> Pin<Box<dyn Future<Output = ActionResult> + Send>> {
        Box::pin(async move {
            match self {
                BrowserAction::CycleStatus { id } => {
                    match TicketService::cycle_status(&id).await {
                        Ok(_) => ActionResult {
                            success: true,
                            message: Some(format!("Status cycled for {}", id)),
                        },
                        Err(e) => ActionResult {
                            success: false,
                            message: Some(format!("Failed to cycle status: {}", e)),
                        },
                    }
                }
                BrowserAction::LoadForEdit { id: _ } => {
                    ActionResult {
                        success: true,
                        message: Some(format!("Loaded for editing")),
                    }
                }
                BrowserAction::MarkTriaged { id, triaged } => {
                    match TicketService::mark_triaged(&id, triaged).await {
                        Ok(_) => ActionResult {
                            success: true,
                            message: Some(format!("Marked {} as triaged: {}", id, triaged)),
                        },
                        Err(e) => ActionResult {
                            success: false,
                            message: Some(format!("Failed to mark as triaged: {}", e)),
                        },
                    }
                }
                BrowserAction::CancelTicket { id } => {
                    match TicketService::set_status(&id, TicketStatus::Cancelled).await {
                        Ok(_) => ActionResult {
                            success: true,
                            message: Some(format!("Cancelled {}", id)),
                        },
                        Err(e) => ActionResult {
                            success: false,
                            message: Some(format!("Failed to cancel ticket: {}", e)),
                        },
                    }
                }
                BrowserAction::AddNote { id, note } => {
                    match TicketService::add_note(&id, &note).await {
                        Ok(_) => ActionResult {
                            success: true,
                            message: Some(format!("Note added to {}", id)),
                        },
                        Err(e) => ActionResult {
                            success: false,
                            message: Some(format!("Failed to add note: {}", e)),
                        },
                    }
                }
            }
        })
    }
}

/// Props for the IssueBrowser component
#[derive(Default, Props)]

/// Process browser actions from the action queue
async fn process_browser_actions(
    actions: Vec<BrowserAction>,
    mut needs_reload: State<bool>,
    mut toast: State<Option<Toast>>,
) {
    let mut success_count = 0;
    let mut errors = Vec::new();

    for action in actions {
        let result = action.execute().await;
        if result.success {
            success_count += 1;
            if let Some(msg) = result.message {
                toast.set(Some(Toast::success(msg)));
            }
        } else if let Some(msg) = result.message {
            errors.push(msg);
        }
    }

    if success_count > 0 {
        needs_reload.set(true);
    }

    if !errors.is_empty() {
        toast.set(Some(Toast::error(format!("{} error(s): {}", errors.len(), errors.join("; ")))))
    }
}

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
    let mut max_detail_scroll = hooks.use_state(|| 0usize);
    let mut active_pane = hooks.use_state(|| Pane::List); // Start on list, not search
    let mut should_exit = hooks.use_state(|| false);
    let mut needs_reload = hooks.use_state(|| false);

    // Triage mode state
    let is_triage_mode = hooks.use_state(|| false);

    // Search state - search is executed on Enter, not while typing
    // Store filtered tickets from search (Vec<FilteredTicket> with highlights)
    let mut search_filtered_tickets: State<Option<Vec<FilteredTicket>>> = hooks.use_state(|| None);
    // Track if search is currently running (for loading indicator)
    let mut search_in_flight = hooks.use_state(|| false);

    // Modal state for triage mode
    let show_note_modal = hooks.use_state(|| false);
    let show_cancel_confirm = hooks.use_state(|| false);
    let note_text: State<String> = hooks.use_state(String::new);
    // Store the ticket info for modals
    let modal_ticket_id: State<String> = hooks.use_state(String::new);
    let modal_ticket_title: State<String> = hooks.use_state(String::new);

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
                        ViewAction::MarkTriaged { id, triaged } => {
                            match TicketService::mark_triaged(&id, triaged).await {
                                Ok(_) => {
                                    should_reload = true;
                                    toast_setter
                                        .set(Some(Toast::success("Ticket marked as triaged")));
                                }
                                Err(e) => {
                                    toast_setter.set(Some(Toast::error(format!(
                                        "Failed to mark as triaged: {}",
                                        e
                                    ))));
                                }
                            }
                        }
                        ViewAction::CancelTicket { id } => {
                            match TicketService::set_status(
                                &id,
                                crate::types::TicketStatus::Cancelled,
                            )
                            .await
                            {
                                Ok(_) => {
                                    should_reload = true;
                                    toast_setter.set(Some(Toast::success("Ticket cancelled")));
                                }
                                Err(e) => {
                                    toast_setter.set(Some(Toast::error(format!(
                                        "Failed to cancel ticket: {}",
                                        e
                                    ))));
                                }
                            }
                        }
                        ViewAction::AddNote { id, note } => {
                            match TicketService::add_note(&id, &note).await {
                                Ok(_) => {
                                    should_reload = true;
                                    toast_setter.set(Some(Toast::success("Note added")));
                                }
                                Err(e) => {
                                    toast_setter.set(Some(Toast::error(format!(
                                        "Failed to add note: {}",
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
    let mut filtered: Vec<FilteredTicket> = if query_str.is_empty() {
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

    // Apply triage mode filter
    if is_triage_mode.get() {
        filtered.retain(|ft| {
            let ticket = &ft.ticket;
            let is_untriaged = ticket.triaged != Some(true);
            let is_new_or_next = matches!(
                ticket.status,
                None | Some(TicketStatus::New) | Some(TicketStatus::Next)
            );
            is_untriaged && is_new_or_next
        });
    }

    let filtered_clone = filtered.clone();

    // Get the currently selected ticket
    let selected_ticket = filtered
        .get(selected_index.get())
        .map(|ft| ft.ticket.clone());

    // Compute max detail scroll (body line count - 1, or 0 if no body)
    let current_max_detail_scroll = if let Some(ref ticket) = selected_ticket {
        if let Some(ref file_path) = ticket.file_path {
            match Ticket::new(file_path.clone()) {
                Ok(ticket_handle) => match ticket_handle.read_content() {
                    Ok(content) => extract_ticket_body(&content)
                        .map(|body| body.lines().count().saturating_sub(1))
                        .unwrap_or(0),
                    Err(_) => 0,
                },
                Err(_) => 0,
            }
        } else {
            0
        }
    } else {
        0
    };

    // Update max_detail_scroll state and reset detail scroll if ticket changed
    if max_detail_scroll.get() != current_max_detail_scroll {
        max_detail_scroll.set(current_max_detail_scroll);
        detail_scroll_offset.set(0);
    }

    // Calculate available height for the list (required for scroll state management)
    // Total height - header (1) - search box (3) - footer (1) - borders (2)
    // NOTE: This calculated value is needed for scroll/navigation logic in handlers
    // and components. The declarative layout uses `height: 100pct` to fill space,
    // but scroll calculations need the actual row count for page-up/down and
    // scroll indicator logic.
    let list_height = height.saturating_sub(7) as usize;

    // Keyboard event handling
    hooks.use_terminal_events({
        let filtered_len = filtered_clone.len();
        let filtered_for_events = filtered_clone.clone();
        let action_sender_for_events = action_sender.clone();
        let is_triage_mode_for_events = is_triage_mode.get();
        let is_editing_for_events = is_editing;
        let show_note_modal_for_events = show_note_modal.get();
        let show_cancel_confirm_for_events = show_cancel_confirm.get();
        let mut is_triage_mode_mut = is_triage_mode;
        let mut show_note_modal_mut = show_note_modal;
        let mut show_cancel_confirm_mut = show_cancel_confirm;
        let mut note_text_mut = note_text;
        let mut modal_ticket_id_mut = modal_ticket_id;
        let mut modal_ticket_title_mut = modal_ticket_title;
        move |event| {
            // Skip if edit form is open (it handles its own events)
            if is_editing_for_events {
                return;
            }

            match event {
                TerminalEvent::Key(KeyEvent {
                    code,
                    kind,
                    modifiers,
                    ..
                }) if kind != KeyEventKind::Release => {
                    // Handle note input modal events
                    if show_note_modal_for_events {
                        match code {
                            KeyCode::Esc => {
                                // Cancel note input
                                show_note_modal_mut.set(false);
                                note_text_mut.set(String::new());
                            }
                            KeyCode::Enter if modifiers == KeyModifiers::NONE => {
                                // Submit note if not empty
                                let note = note_text_mut.to_string();
                                if !note.trim().is_empty() {
                                    let id = modal_ticket_id_mut.to_string();
                                    let _ = action_sender_for_events
                                        .send(ViewAction::AddNote { id, note });
                                    actions_pending.set(true);
                                }
                                show_note_modal_mut.set(false);
                                note_text_mut.set(String::new());
                            }
                            _ => {
                                // Let the modal's TextInput handle other keys
                            }
                        }
                        return;
                    }

                    // Handle cancel confirmation modal events
                    if show_cancel_confirm_for_events {
                        if code == KeyCode::Char('c') {
                            // Confirm cancellation
                            let id = modal_ticket_id_mut.to_string();
                            let _ = action_sender_for_events.send(ViewAction::CancelTicket { id });
                            actions_pending.set(true);
                        }
                        // Any key (including 'c' after confirming) closes the modal
                        show_cancel_confirm_mut.set(false);
                        return;
                    }

                    // Handle Ctrl+T to toggle triage mode
                    if code == KeyCode::Char('t') && modifiers == KeyModifiers::CONTROL {
                        // Toggle triage mode
                        is_triage_mode_mut.set(!is_triage_mode_for_events);
                        return;
                    }

                    // Handle triage mode modal triggers (before passing to handler)
                    if is_triage_mode_for_events {
                        if code == KeyCode::Char('n') {
                            // Open note input modal
                            if let Some(ft) = filtered_for_events.get(selected_index.get())
                                && let Some(id) = &ft.ticket.id
                            {
                                modal_ticket_id_mut.set(id.clone());
                                modal_ticket_title_mut
                                    .set(ft.ticket.title.clone().unwrap_or_default());
                                note_text_mut.set(String::new());
                                show_note_modal_mut.set(true);
                            }
                            return;
                        }
                        if code == KeyCode::Char('c') {
                            // Open cancel confirmation modal
                            if let Some(ft) = filtered_for_events.get(selected_index.get())
                                && let Some(id) = &ft.ticket.id
                            {
                                modal_ticket_id_mut.set(id.clone());
                                modal_ticket_title_mut
                                    .set(ft.ticket.title.clone().unwrap_or_default());
                                show_cancel_confirm_mut.set(true);
                            }
                            return;
                        }
                    }

                    let mut ctx = handlers::ViewHandlerContext {
                        search_query: &mut search_query,
                        pending_search: &mut pending_search,
                        selected_index: &mut selected_index,
                        scroll_offset: &mut scroll_offset,
                        detail_scroll_offset: &mut detail_scroll_offset,
                        active_pane: &mut active_pane,
                        is_triage_mode: is_triage_mode_for_events,
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
                        max_detail_scroll: max_detail_scroll.get(),
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

    // Determine shortcuts to show - check modals first, then normal modes
    let shortcuts = if show_note_modal.get() {
        // Triage mode: note input modal is open
        note_input_modal_shortcuts()
    } else if show_cancel_confirm.get() {
        // Triage mode: cancel confirmation modal is open
        cancel_confirm_modal_shortcuts()
    } else if is_editing {
        edit_shortcuts()
    } else if show_full_empty_state {
        empty_shortcuts()
    } else if is_triage_mode.get() {
        triage_shortcuts()
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
            position: Position::Relative,
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
                        overflow: Overflow::Hidden,
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
                                    overflow: Overflow::Hidden,
                                ) {
                                    // Left pane: Ticket list (35% width via declarative flexbox)
                                    View(
                                        width: 35pct,
                                        height: 100pct,
                                        flex_shrink: 0.0,
                                    ) {
                                        TicketList(
                                            tickets: filtered.clone(),
                                            selected_index: selected_index.get(),
                                            scroll_offset: scroll_offset.get(),
                                            has_focus: active_pane.get() == Pane::List && !is_editing,
                                            visible_height: list_height,
                                            searching: search_in_flight.get(),
                                        )
                                    }

                                    // Right pane: Ticket detail (takes remaining 65% via declarative flexbox)
                                    View(
                                        flex_grow: 1.0,
                                        height: 100pct,
                                    ) {
                                        TicketDetail(
                                            ticket: selected_ticket.clone(),
                                            has_focus: active_pane.get() == Pane::Detail && !is_editing,
                                            scroll_offset: detail_scroll_offset.get(),
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
                    EditFormOverlay(
                        ticket: edit_ticket.clone(),
                        initial_body: edit_body.clone(),
                        on_close: Some(edit_result),
                    )
                })
            } else {
                None
            })

            // Note input modal (triage mode)
            #(if show_note_modal.get() {
                Some(element! {
                    NoteInputModal(
                        ticket_id: modal_ticket_id.to_string(),
                        note_text: Some(note_text),
                    )
                })
            } else {
                None
            })

            // Cancel confirmation modal (triage mode)
            #(if show_cancel_confirm.get() {
                Some(element! {
                    CancelConfirmModal(
                        ticket_id: modal_ticket_id.to_string(),
                        ticket_title: modal_ticket_title.to_string(),
                    )
                })
            } else {
                None
            })
        }
    }
}
