//! Issue browser view (`janus view`)
//!
//! Provides an interactive TUI for browsing and managing tickets with
//! fuzzy search, keyboard navigation, and inline detail viewing.

pub mod handlers;
pub mod modals;
pub mod model;

use iocraft::prelude::*;

use crate::formatting::extract_ticket_body;
use crate::ticket::Ticket;
use crate::tui::components::{
    CacheErrorModalData, Clickable, EmptyState, EmptyStateKind, ModalState, NoteModalData,
    SearchBox, TicketDetail, TicketList, TicketModalData, Toast, browser_shortcuts,
    cancel_confirm_modal_shortcuts, compute_empty_state, edit_shortcuts, empty_shortcuts,
    error_modal_shortcuts, note_input_modal_shortcuts, search_shortcuts, triage_shortcuts,
};
use crate::tui::edit::{EditFormOverlay, EditResult};
use crate::tui::edit_state::{EditFormState, EditMode};
use crate::tui::hooks::use_ticket_loader;
use crate::tui::repository::InitResult;
use crate::tui::screen_base::{ScreenLayout, calculate_list_height, should_process_key_event};
use crate::tui::search_orchestrator::{SearchState, compute_filtered_tickets};
use crate::tui::services::TicketService;
use crate::tui::state::Pane;
use crate::types::TicketMetadata;

use modals::{CacheErrorModal, CancelConfirmModal, NoteInputModal};

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
    let mut toast: State<Option<Toast>> = hooks.use_state(|| None);
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
    let mut search_state = SearchState::use_state(&mut hooks);

    // Modal state for triage mode using generic ModalState
    let note_modal = ModalState::<NoteModalData>::use_state(&mut hooks);
    let cancel_confirm_modal = ModalState::<TicketModalData>::use_state(&mut hooks);
    let cache_error_modal = ModalState::<CacheErrorModalData>::use_state(&mut hooks);

    // Async load handler with minimum 100ms display time to prevent UI flicker
    let load_handler: Handler<()> =
        hooks.use_async_handler(use_ticket_loader(all_tickets, is_loading, init_result));

    // Trigger initial load on mount
    let mut load_started = hooks.use_state(|| false);
    if !load_started.get() {
        load_started.set(true);
        load_handler.clone()(());
    }

    // Edit form state - single enum tracks the editing mode
    let mut edit_mode: State<EditMode> = hooks.use_state(EditMode::default);
    let mut edit_result: State<EditResult> = hooks.use_state(EditResult::default);

    // Direct async handlers for ticket operations (replaces action queue pattern)

    // Cycle status handler
    let cycle_status_handler: Handler<String> = hooks.use_async_handler({
        let toast_setter = toast;
        let all_tickets_setter = all_tickets;
        let cache_error_modal_setter = cache_error_modal;
        move |ticket_id: String| {
            let mut toast_setter = toast_setter;
            let mut all_tickets_setter = all_tickets_setter;
            let cache_error_modal_setter = cache_error_modal_setter;
            async move {
                match TicketService::cycle_status(&ticket_id).await {
                    Ok(_) => {
                        toast_setter.set(Some(Toast::success(format!(
                            "Status cycled for {}",
                            ticket_id
                        ))));
                        // Sync cache and reload tickets
                        if let Err(e) = crate::cache::sync_cache().await {
                            cache_error_modal_setter
                                .open(CacheErrorModalData::new(format!("{}", e)));
                            return;
                        }
                        let tickets =
                            crate::tui::repository::TicketRepository::load_tickets().await;
                        all_tickets_setter.set(tickets);
                    }
                    Err(e) => {
                        toast_setter
                            .set(Some(Toast::error(format!("Failed to cycle status: {}", e))));
                    }
                }
            }
        }
    });

    // Mark triaged handler
    let mark_triaged_handler: Handler<(String, bool)> = hooks.use_async_handler({
        let toast_setter = toast;
        let all_tickets_setter = all_tickets;
        let cache_error_modal_setter = cache_error_modal;
        move |(ticket_id, triaged): (String, bool)| {
            let mut toast_setter = toast_setter;
            let mut all_tickets_setter = all_tickets_setter;
            let cache_error_modal_setter = cache_error_modal_setter;
            async move {
                match TicketService::mark_triaged(&ticket_id, triaged).await {
                    Ok(_) => {
                        let msg = if triaged {
                            format!("Marked {} as triaged", ticket_id)
                        } else {
                            format!("Unmarked {} as triaged", ticket_id)
                        };
                        toast_setter.set(Some(Toast::success(msg)));
                        // Sync cache and reload tickets
                        if let Err(e) = crate::cache::sync_cache().await {
                            cache_error_modal_setter
                                .open(CacheErrorModalData::new(format!("{}", e)));
                            return;
                        }
                        let tickets =
                            crate::tui::repository::TicketRepository::load_tickets().await;
                        all_tickets_setter.set(tickets);
                    }
                    Err(e) => {
                        toast_setter.set(Some(Toast::error(format!(
                            "Failed to mark as triaged: {}",
                            e
                        ))));
                    }
                }
            }
        }
    });

    // Cancel ticket handler
    let cancel_ticket_handler: Handler<String> = hooks.use_async_handler({
        let toast_setter = toast;
        let all_tickets_setter = all_tickets;
        let cache_error_modal_setter = cache_error_modal;
        move |ticket_id: String| {
            let mut toast_setter = toast_setter;
            let mut all_tickets_setter = all_tickets_setter;
            let cache_error_modal_setter = cache_error_modal_setter;
            async move {
                match TicketService::set_status(&ticket_id, crate::types::TicketStatus::Cancelled)
                    .await
                {
                    Ok(_) => {
                        toast_setter.set(Some(Toast::success(format!("Cancelled {}", ticket_id))));
                        // Sync cache and reload tickets
                        if let Err(e) = crate::cache::sync_cache().await {
                            cache_error_modal_setter
                                .open(CacheErrorModalData::new(format!("{}", e)));
                            return;
                        }
                        let tickets =
                            crate::tui::repository::TicketRepository::load_tickets().await;
                        all_tickets_setter.set(tickets);
                    }
                    Err(e) => {
                        toast_setter.set(Some(Toast::error(format!(
                            "Failed to cancel ticket: {}",
                            e
                        ))));
                    }
                }
            }
        }
    });

    // Add note handler
    let add_note_handler: Handler<(String, String)> = hooks.use_async_handler({
        let toast_setter = toast;
        let all_tickets_setter = all_tickets;
        let cache_error_modal_setter = cache_error_modal;
        move |(ticket_id, note): (String, String)| {
            let mut toast_setter = toast_setter;
            let mut all_tickets_setter = all_tickets_setter;
            let cache_error_modal_setter = cache_error_modal_setter;
            async move {
                match TicketService::add_note(&ticket_id, &note).await {
                    Ok(_) => {
                        toast_setter
                            .set(Some(Toast::success(format!("Added note to {}", ticket_id))));
                        // Sync cache and reload tickets
                        if let Err(e) = crate::cache::sync_cache().await {
                            cache_error_modal_setter
                                .open(CacheErrorModalData::new(format!("{}", e)));
                            return;
                        }
                        let tickets =
                            crate::tui::repository::TicketRepository::load_tickets().await;
                        all_tickets_setter.set(tickets);
                    }
                    Err(e) => {
                        toast_setter.set(Some(Toast::error(format!("Failed to add note: {}", e))));
                    }
                }
            }
        }
    });

    // Pane focus handlers - created at top level to follow rules of hooks
    let focus_search_handler: Handler<()> = hooks.use_async_handler({
        let active_pane_setter = active_pane;
        move |()| {
            let mut active_pane_setter = active_pane_setter;
            async move {
                active_pane_setter.set(Pane::Search);
            }
        }
    });

    let focus_list_handler: Handler<()> = hooks.use_async_handler({
        let active_pane_setter = active_pane;
        move |()| {
            let mut active_pane_setter = active_pane_setter;
            async move {
                active_pane_setter.set(Pane::List);
            }
        }
    });

    let focus_detail_handler: Handler<()> = hooks.use_async_handler({
        let active_pane_setter = active_pane;
        move |()| {
            let mut active_pane_setter = active_pane_setter;
            async move {
                active_pane_setter.set(Pane::Detail);
            }
        }
    });

    // TicketList row click handler - created at top level to follow rules of hooks
    let row_click_handler: Handler<usize> = hooks.use_async_handler({
        let selected_setter = selected_index;
        let pane_setter = active_pane;
        let scroll_setter = scroll_offset;
        move |idx: usize| {
            let mut selected_setter = selected_setter;
            let mut pane_setter = pane_setter;
            let mut scroll_setter = scroll_setter;
            async move {
                selected_setter.set(idx);
                pane_setter.set(Pane::List);
                // Update scroll offset if needed to keep selection visible
                if idx < scroll_setter.get() {
                    scroll_setter.set(idx);
                }
            }
        }
    });

    // TicketList scroll handlers - created at top level to follow rules of hooks
    let list_scroll_up_handler: Handler<()> = hooks.use_async_handler({
        let scroll_setter = scroll_offset;
        move |()| {
            let mut scroll_setter = scroll_setter;
            async move {
                // Scroll up: decrease offset by 3 items
                scroll_setter.set(scroll_setter.get().saturating_sub(3));
            }
        }
    });

    let list_scroll_down_handler: Handler<()> = hooks.use_async_handler({
        let scroll_setter = scroll_offset;
        move |()| {
            let mut scroll_setter = scroll_setter;
            async move {
                // Scroll down: increase offset by 3 items
                // The view handles clamping to valid range
                scroll_setter.set(scroll_setter.get() + 3);
            }
        }
    });

    // Detail pane scroll handlers - created at top level to follow rules of hooks
    let detail_scroll_up_handler: Handler<()> = hooks.use_async_handler({
        let scroll_setter = detail_scroll_offset;
        move |()| {
            let mut scroll_setter = scroll_setter;
            async move {
                // Scroll up: decrease offset by 3 lines
                scroll_setter.set(scroll_setter.get().saturating_sub(3));
            }
        }
    });

    let detail_scroll_down_handler: Handler<()> = hooks.use_async_handler({
        let scroll_setter = detail_scroll_offset;
        let max_scroll_ref = max_detail_scroll;
        move |()| {
            let mut scroll_setter = scroll_setter;
            let max_scroll = max_scroll_ref.get();
            async move {
                // Scroll down: increase offset by 3 lines, capped at max
                scroll_setter.set((scroll_setter.get() + 3).min(max_scroll));
            }
        }
    });

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

    // Check if edit form is open
    let is_editing = !matches!(*edit_mode.read(), EditMode::None);

    // Compute filtered tickets
    let query_str = search_query.to_string();

    search_state.check_pending(query_str.clone());
    search_state.clear_if_empty(&query_str);

    // Check for semantic search errors and display toast if present
    // Only show error if user explicitly requested semantic search with ~ prefix
    if query_str.starts_with('~')
        && let Some(error) = search_state.take_semantic_error()
    {
        // Provide user-friendly error message
        let user_message = format!("Semantic search failed: {}", error);
        toast.set(Some(Toast::error(user_message)));
    }

    let filtered = compute_filtered_tickets(&all_tickets.read(), &search_state, &query_str);

    // Clone filtered for event handler closure (each clone is cheap since FilteredTicket contains Arc)
    let filtered_for_handlers = filtered.clone();

    // Clone search state refs for event handler
    let search_in_flight_ref = search_state.in_flight;

    // Get the currently selected ticket
    let selected_ticket = filtered
        .get(selected_index.get())
        .map(|ft| ft.ticket.as_ref().clone());

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

    // Clone handlers for use in event handler closure
    let cycle_status_handler_for_events = cycle_status_handler.clone();
    let mark_triaged_handler_for_events = mark_triaged_handler.clone();
    let cancel_ticket_handler_for_events = cancel_ticket_handler.clone();
    let add_note_handler_for_events = add_note_handler.clone();

    // Keyboard event handling
    hooks.use_terminal_events({
        let filtered_len = filtered_for_handlers.len();
        let filtered_for_events = filtered_for_handlers.clone();
        let is_triage_mode_for_events = is_triage_mode;
        let edit_mode_for_events = edit_mode;
        let note_modal_open = note_modal.is_open();
        let cancel_confirm_open = cancel_confirm_modal.is_open();
        let cache_error_open = cache_error_modal.is_open();
        let mut is_triage_mode_mut = is_triage_mode;
        let mut should_exit_for_events = should_exit;
        move |event| {
            // Skip if edit form is open (it handles its own events)
            if !matches!(*edit_mode_for_events.read(), EditMode::None) {
                return;
            }

            match event {
                TerminalEvent::Key(KeyEvent {
                    code,
                    kind,
                    modifiers,
                    ..
                }) if should_process_key_event(kind) => {
                    // Handle cache error modal events - any key closes and exits
                    if cache_error_open {
                        cache_error_modal.close();
                        should_exit_for_events.set(true);
                        return;
                    }

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
                                    add_note_handler_for_events.clone()((
                                        data.ticket_id,
                                        data.text,
                                    ));
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
                            cancel_ticket_handler_for_events.clone()(data.ticket_id);
                        }
                        // Any key (including 'c' after confirming) closes the modal
                        cancel_confirm_modal.close();
                        return;
                    }

                    // Handle Ctrl+T to toggle triage mode
                    if code == KeyCode::Char('t') && modifiers == KeyModifiers::CONTROL {
                        // Toggle triage mode
                        is_triage_mode_mut.set(!is_triage_mode_for_events.get());
                        return;
                    }

                    // Handle triage mode modal triggers (before passing to handler)
                    if is_triage_mode_for_events.get() {
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
                        search: handlers::SearchState {
                            query: &mut search_query,
                            orchestrator: &mut search_state,
                        },
                        app: handlers::AppState {
                            should_exit: &mut should_exit,
                            needs_reload: &mut needs_reload,
                            active_pane: &mut active_pane,
                            is_triage_mode: is_triage_mode_for_events.get(),
                            toast_setter: Some(&mut toast),
                        },
                        data: handlers::ViewData {
                            filtered_tickets: &filtered_for_events,
                            filtered_count: filtered_len,
                            list_height,
                            list_nav: handlers::ListNavigationState {
                                selected_index: &mut selected_index,
                                scroll_offset: &mut scroll_offset,
                            },
                            detail_nav: handlers::DetailNavigationState {
                                scroll_offset: &mut detail_scroll_offset,
                                max_scroll: max_detail_scroll.get(),
                            },
                        },
                        edit: handlers::EditState {
                            mode: &mut edit_mode,
                            result: &mut edit_result,
                        },
                        handlers: handlers::AsyncHandlers {
                            cycle_status: &cycle_status_handler_for_events,
                            mark_triaged: &mark_triaged_handler_for_events,
                        },
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

    // Reset selection if it's out of bounds after filtering
    if selected_index.get() >= filtered.len() && !filtered.is_empty() {
        selected_index.set(filtered.len() - 1);
    }
    if scroll_offset.get() > selected_index.get() {
        // Only constrain scroll_offset when it would hide the selected item above viewport
        // Allow scroll_offset > selected_index so users can scroll down to see more tickets
        // while keeping their current selection visible (or just off-screen)
        let viewport_size = list_height.saturating_sub(2).max(1);
        if selected_index.get() >= scroll_offset.get() + viewport_size {
            // Selected item is below viewport, scroll down to keep it visible
            scroll_offset.set(selected_index.get().saturating_sub(viewport_size - 1));
        }
    }

    let ticket_count = filtered.len();
    let tickets_ref_for_count = all_tickets.read();
    let total_ticket_count = tickets_ref_for_count.len();
    drop(tickets_ref_for_count);

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
    let shortcuts = if cache_error_modal.is_open() {
        // Cache error modal is open
        error_modal_shortcuts()
    } else if note_modal.is_open() {
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

    // Build triage action buttons when in triage mode and no modal is open
    let triage_action_buttons: Vec<AnyElement<'_>> =
        if is_triage_mode.get()
            && !note_modal.is_open()
            && !cancel_confirm_modal.is_open()
            && !is_editing
            && !show_full_empty_state
        {
            // Get the currently selected ticket for status-based action availability
            let selected_ticket_status = filtered
                .get(selected_index.get())
                .and_then(|ft| ft.ticket.status);

            let mut buttons: Vec<AnyElement<'_>> = Vec::new();

            // Mark Triaged button (only for non-cancelled tickets)
            let can_mark_triaged = selected_ticket_status
                .map(|s| s != crate::types::TicketStatus::Cancelled)
                .unwrap_or(true);
            if can_mark_triaged
                && let Some(ft) = filtered.get(selected_index.get())
                && let Some(id) = &ft.ticket.id
            {
                let id = id.clone();
                let mark_handler = mark_triaged_handler.clone();
                buttons.push(
                    element! {
                        Button(
                            handler: move |_| mark_handler((id.clone(), true)),
                            has_focus: false,
                        ) {
                            View(
                                border_style: BorderStyle::Round,
                                border_color: Color::Green,
                                padding_left: 1,
                                padding_right: 1,
                                background_color: Color::Green,
                            ) {
                                Text(
                                    content: "[t] Triaged",
                                    color: Color::Black,
                                    weight: Weight::Bold,
                                )
                            }
                        }
                    }
                    .into(),
                );
            }

            // Add Note button (always available)
            if let Some(ft) = filtered.get(selected_index.get())
                && let Some(id) = &ft.ticket.id
            {
                let id = id.clone();
                let note_modal_ref = note_modal;
                buttons.push(
                    element! {
                        Button(
                            handler: move |_| {
                                note_modal_ref.open(NoteModalData::new(id.clone()));
                            },
                            has_focus: false,
                        ) {
                            View(
                                border_style: BorderStyle::Round,
                                border_color: Color::Blue,
                                padding_left: 1,
                                padding_right: 1,
                                background_color: Color::Blue,
                            ) {
                                Text(
                                    content: "[n] Note",
                                    color: Color::White,
                                    weight: Weight::Bold,
                                )
                            }
                        }
                    }
                    .into(),
                );
            }

            // Change Status button (cycle status)
            if let Some(ft) = filtered.get(selected_index.get())
                && let Some(id) = &ft.ticket.id
            {
                let id = id.clone();
                let cycle_handler = cycle_status_handler.clone();
                buttons.push(
                    element! {
                        Button(
                            handler: move |_| cycle_handler(id.clone()),
                            has_focus: false,
                        ) {
                            View(
                                border_style: BorderStyle::Round,
                                border_color: Color::Cyan,
                                padding_left: 1,
                                padding_right: 1,
                                background_color: Color::Cyan,
                            ) {
                                Text(
                                    content: "[s] Status",
                                    color: Color::Black,
                                    weight: Weight::Bold,
                                )
                            }
                        }
                    }
                    .into(),
                );
            }

            // Cancel button (only for non-cancelled tickets)
            let can_cancel = selected_ticket_status
                .map(|s| s != crate::types::TicketStatus::Cancelled)
                .unwrap_or(true);
            if can_cancel
                && let Some(ft) = filtered.get(selected_index.get())
                && let Some(id) = &ft.ticket.id
            {
                let id = id.clone();
                let title = ft.ticket.title.clone().unwrap_or_default();
                let cancel_modal_ref = cancel_confirm_modal;
                buttons.push(element! {
                Button(
                    handler: move |_| {
                        cancel_modal_ref.open(TicketModalData::new(id.clone(), title.clone()));
                    },
                    has_focus: false,
                ) {
                    View(
                        border_style: BorderStyle::Round,
                        border_color: Color::Grey,
                        padding_left: 1,
                        padding_right: 1,
                        background_color: Color::Grey,
                    ) {
                        Text(
                            content: "[c] Cancel",
                            color: Color::White,
                            weight: Weight::Bold,
                        )
                    }
                }
            }.into());
            }

            buttons
        } else {
            Vec::new()
        };

    element! {
        ScreenLayout(
            width: width,
            height: height,
            header_subtitle: Some("Browser"),
            header_ticket_count: Some(ticket_count),
            shortcuts: shortcuts,
            action_buttons: triage_action_buttons,
            toast: toast.read().clone(),
            triage_mode: is_triage_mode.get(),
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
                        // Search box with clickable focus
                        Clickable(
                            on_click: Some(focus_search_handler.clone()),
                        ) {
                            View(
                                width: 100pct,
                                padding_left: 1,
                                padding_right: 1,
                            ) {
                                SearchBox(
                                    value: Some(search_query),
                                    has_focus: active_pane.get() == Pane::Search && !is_editing,
                                    is_semantic: query_str.starts_with('~'),
                                )
                            }
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
                                    Clickable(
                                        on_click: Some(focus_list_handler.clone()),
                                        on_scroll_up: Some(list_scroll_up_handler.clone()),
                                        on_scroll_down: Some(list_scroll_down_handler.clone()),
                                    ) {
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
                                                searching: search_in_flight_ref.get(),
                                                on_row_click: Some(row_click_handler.clone()),
                                            )
                                        }
                                    }

                                    // Right pane: Ticket detail (takes remaining 65% via declarative flexbox)
                                    Clickable(
                                        on_click: Some(focus_detail_handler.clone()),
                                    ) {
                                        View(
                                            flex_grow: 1.0,
                                            height: 100pct,
                                        ) {
                                            TicketDetail(
                                                ticket: selected_ticket.clone(),
                                                has_focus: active_pane.get() == Pane::Detail && !is_editing,
                                                scroll_offset: detail_scroll_offset.get(),
                                                on_scroll_up: Some(detail_scroll_up_handler.clone()),
                                                on_scroll_down: Some(detail_scroll_down_handler.clone()),
                                            )
                                        }
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

            // Cache error modal
            #(if cache_error_modal.is_open() {
                let data = cache_error_modal.data();
                Some(element! {
                    CacheErrorModal(
                        error_message: data.error_message.clone(),
                    )
                })
            } else {
                None
            })
        }
    }
}
