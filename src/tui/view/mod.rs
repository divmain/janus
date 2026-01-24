//! Issue browser view (`janus view`)
//!
//! Provides an interactive TUI for browsing and managing tickets with
//! fuzzy search, keyboard navigation, and inline detail viewing.

pub mod handlers;
pub mod modals;
pub mod model;

use iocraft::prelude::*;
use std::pin::Pin;

use crate::formatting::extract_ticket_body;
use crate::ticket::Ticket;
use crate::tui::action_queue::{Action, ActionQueueBuilder};
use crate::tui::components::{
    EmptyState, EmptyStateKind, ModalState, NoteModalData, SearchBox, TicketDetail, TicketList,
    TicketModalData, Toast, browser_shortcuts, cancel_confirm_modal_shortcuts, compute_empty_state,
    edit_shortcuts, empty_shortcuts, note_input_modal_shortcuts, search_shortcuts,
    triage_shortcuts,
};
use crate::tui::edit::{EditFormOverlay, EditResult};
use crate::tui::edit_state::EditFormState;
use crate::tui::hooks::use_ticket_loader;
use crate::tui::repository::InitResult;
use crate::tui::screen_base::{ScreenLayout, calculate_list_height, should_process_key_event};
use crate::tui::search::{FilteredTicket, compute_title_highlights};
use crate::tui::state::Pane;
use crate::types::{TicketMetadata, TicketStatus};

use handlers::ViewAction;
use modals::{CancelConfirmModal, NoteInputModal};

/// Props for the IssueBrowser component
#[derive(Default, Props)]
pub struct IssueBrowserProps {}

/// Process browser actions from the action queue
async fn process_browser_actions(
    actions: Vec<ViewAction>,
    mut needs_reload: State<bool>,
    mut toast: State<Option<Toast>>,
    mut editing_ticket_id: State<String>,
    mut editing_ticket: State<TicketMetadata>,
    mut editing_body: State<String>,
    mut is_editing: State<bool>,
) {
    use crate::tui::action_queue::ActionResult;

    let mut success_count = 0;
    let mut errors = Vec::new();

    for action in actions {
        let result = action.execute().await;

        match result {
            ActionResult::LoadForEdit { success, id: _action_id, metadata, body, message } => {
                if success {
                    let ticket_id = metadata.id.clone().unwrap_or_default();
                    let ticket_metadata = TicketMetadata {
                        id: metadata.id,
                        uuid: metadata.uuid,
                        title: metadata.title,
                        status: metadata.status,
                        ticket_type: metadata.ticket_type,
                        priority: metadata.priority,
                        triaged: metadata.triaged,
                        created: metadata.created,
                        file_path: metadata.file_path.map(std::path::PathBuf::from),
                        deps: metadata.deps,
                        links: metadata.links,
                        external_ref: metadata.external_ref,
                        remote: metadata.remote,
                        parent: metadata.parent,
                        spawned_from: metadata.spawned_from,
                        spawn_context: metadata.spawn_context,
                        depth: metadata.depth,
                        completion_summary: metadata.completion_summary,
                        body: None,
                    };
                    editing_ticket_id.set(ticket_id);
                    editing_ticket.set(ticket_metadata);
                    editing_body.set(body);
                    is_editing.set(true);
                    success_count += 1;
                    if let Some(msg) = message {
                        toast.set(Some(Toast::success(msg)));
                    }
                } else if let Some(msg) = message {
                    errors.push(msg);
                }
            }
            ActionResult::Result { success, message } => {
                if success {
                    success_count += 1;
                    if let Some(msg) = message {
                        toast.set(Some(Toast::success(msg)));
                    }
                } else if let Some(msg) = message {
                    errors.push(msg);
                }
            }
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
    let mut active_pane = hooks.use_state(|| Pane::List);
    let mut should_exit = hooks.use_state(|| false);
    let mut needs_reload = hooks.use_state(|| false);

    // Triage mode state
    let is_triage_mode = hooks.use_state(|| false);

    // Search state - search is executed on Enter, not while typing
    // Store filtered tickets from search (Vec<FilteredTicket> with highlights)
    let mut search_filtered_tickets: State<Option<Vec<FilteredTicket>>> = hooks.use_state(|| None);
    // Track if search is currently running (for loading indicator)
    let mut search_in_flight = hooks.use_state(|| false);

    // Modal state for triage mode using generic ModalState
    // Marker types for type-level modal distinction
    struct NoteModalMarker;
    struct CancelConfirmModalMarker;

    let note_modal = ModalState::<NoteModalMarker, NoteModalData>::use_state(&mut hooks);
    let cancel_confirm_modal =
        ModalState::<CancelConfirmModalMarker, TicketModalData>::use_state(&mut hooks);

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

    // Action queue for async ticket operations using ActionQueueBuilder
    let process_fn = {
        let editing_ticket_id = editing_ticket_id;
        let editing_ticket = editing_ticket;
        let editing_body = editing_body;
        let is_editing = is_editing_existing;
        move |actions, needs_reload, toast| -> Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
            Box::pin(process_browser_actions(
                actions,
                needs_reload,
                toast,
                editing_ticket_id,
                editing_ticket,
                editing_body,
                is_editing,
            ))
        }
    };

    let (_queue_state, _action_handler, action_channel) = ActionQueueBuilder::use_state(
        &mut hooks,
        process_fn,
        needs_reload,
        toast,
    );

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
    // Additional elements: search box (3) + borders (2) = 5
    // NOTE: This calculated value is needed for scroll/navigation logic in handlers
    // and components. The declarative layout uses `height: 100pct` to fill space,
    // but scroll calculations need the actual row count for page-up/down and
    // scroll indicator logic.
    let list_height = calculate_list_height(height, 5);

    // Keyboard event handling
    hooks.use_terminal_events({
        let filtered_len = filtered_clone.len();
        let filtered_for_events = filtered_clone.clone();
        let action_channel_for_events = action_channel.clone();
        let is_triage_mode_for_events = is_triage_mode.get();
        let is_editing_for_events = is_editing;
        let note_modal_open = note_modal.is_open();
        let cancel_confirm_open = cancel_confirm_modal.is_open();
        let mut is_triage_mode_mut = is_triage_mode;
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
                }) if should_process_key_event(kind) => {
                    // Handle note input modal events
                    if note_modal_open {
                        match code {
                            KeyCode::Esc => {
                                // Cancel note input
                                note_modal.close();
                            }
                            KeyCode::Enter if modifiers == KeyModifiers::NONE => {
                                // Submit note if not empty
                                let data = note_modal.data();
                                if !data.text.trim().is_empty() {
                                    let _ = action_channel_for_events.send(ViewAction::AddNote {
                                        id: data.ticket_id,
                                        note: data.text,
                                    });
                                    actions_pending.set(true);
                                }
                                note_modal.close();
                            }
                            _ => {
                                // Let the modal's TextInput handle other keys
                            }
                        }
                        return;
                    }

                    // Handle cancel confirmation modal events
                    if cancel_confirm_open {
                        if code == KeyCode::Char('c') {
                            // Confirm cancellation
                            let data = cancel_confirm_modal.data();
                            let _ = action_channel_for_events
                                .send(ViewAction::CancelTicket { id: data.ticket_id });
                            actions_pending.set(true);
                        }
                        // Any key (including 'c' after confirming) closes the modal
                        cancel_confirm_modal.close();
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
                                note_modal.open(NoteModalData::new(id.clone()));
                            }
                            return;
                        }
                        if code == KeyCode::Char('c') {
                            // Open cancel confirmation modal
                            if let Some(ft) = filtered_for_events.get(selected_index.get())
                                && let Some(id) = &ft.ticket.id
                            {
                                cancel_confirm_modal.open(TicketModalData::new(
                                    id.clone(),
                                    ft.ticket.title.clone().unwrap_or_default(),
                                ));
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
                        action_tx: &action_channel_for_events,
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
    let shortcuts = if note_modal.is_open() {
        // Triage mode: note input modal is open
        note_input_modal_shortcuts()
    } else if cancel_confirm_modal.is_open() {
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
        ScreenLayout(
            width: width,
            height: height,
            header_subtitle: Some("Browser"),
            header_ticket_count: Some(ticket_count),
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
            #(if note_modal.is_open() {
                let data = note_modal.data();
                Some(element! {
                    NoteInputModal(
                        ticket_id: data.ticket_id.clone(),
                        note_text: Some(note_modal.data_state()),
                    )
                })
            } else {
                None
            })

            // Cancel confirmation modal (triage mode)
            #(if cancel_confirm_modal.is_open() {
                let data = cancel_confirm_modal.data();
                Some(element! {
                    CancelConfirmModal(
                        ticket_id: data.ticket_id.clone(),
                        ticket_title: data.ticket_title.clone(),
                    )
                })
            } else {
                None
            })
        }
    }
}
