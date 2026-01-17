//! Issue browser view (`janus view`)
//!
//! Provides an interactive TUI for browsing and managing tickets with
//! fuzzy search, keyboard navigation, and inline detail viewing.

use iocraft::prelude::*;

use crate::tui::components::{
    EmptyState, EmptyStateKind, Footer, Header, SearchBox, browser_shortcuts, edit_shortcuts,
    empty_shortcuts, search_shortcuts,
};
use crate::tui::edit::{EditForm, EditResult};
use crate::tui::search::{FilteredTicket, filter_tickets};
use crate::tui::services::TicketService;
use crate::tui::state::{InitResult, Pane, TuiState};
use crate::tui::theme::theme;
use crate::types::TicketMetadata;

use super::components::{TicketDetail, TicketList};

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

    // State management
    let init_result: State<InitResult> = hooks.use_state(|| TuiState::init_sync().1);
    let mut all_tickets: State<Vec<TicketMetadata>> =
        hooks.use_state(|| TuiState::new_sync().all_tickets);
    let mut search_query = hooks.use_state(String::new);
    let mut selected_index = hooks.use_state(|| 0usize);
    let mut scroll_offset = hooks.use_state(|| 0usize);
    let mut active_pane = hooks.use_state(|| Pane::List); // Start on list, not search
    let mut should_exit = hooks.use_state(|| false);
    let mut needs_reload = hooks.use_state(|| false);

    // Edit form state - use bool flags and separate storage for non-Copy data
    let mut edit_result: State<EditResult> = hooks.use_state(EditResult::default);
    let mut is_editing_existing = hooks.use_state(|| false);
    let mut is_creating_new = hooks.use_state(|| false);
    let mut editing_ticket_id = hooks.use_state(String::new);
    let mut editing_ticket: State<TicketMetadata> = hooks.use_state(TicketMetadata::default);
    let mut editing_body = hooks.use_state(String::new);

    // Reload tickets if needed
    if needs_reload.get() {
        needs_reload.set(false);
        all_tickets.set(TuiState::new_sync().all_tickets);
    }

    // Handle edit form result
    match edit_result.get() {
        EditResult::Saved => {
            edit_result.set(EditResult::Editing);
            is_editing_existing.set(false);
            is_creating_new.set(false);
            editing_ticket_id.set(String::new());
            editing_ticket.set(TicketMetadata::default());
            editing_body.set(String::new());
            needs_reload.set(true);
        }
        EditResult::Cancelled => {
            edit_result.set(EditResult::Editing);
            is_editing_existing.set(false);
            is_creating_new.set(false);
            editing_ticket_id.set(String::new());
            editing_ticket.set(TicketMetadata::default());
            editing_body.set(String::new());
        }
        EditResult::Editing => {}
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
                    match active_pane.get() {
                        Pane::Search => {
                            match code {
                                KeyCode::Esc => {
                                    // Clear search and switch to list
                                    search_query.set(String::new());
                                    active_pane.set(Pane::List);
                                }
                                KeyCode::Enter => {
                                    // Switch to list pane after searching
                                    active_pane.set(Pane::List);
                                }
                                KeyCode::Tab => {
                                    active_pane.set(Pane::List);
                                }
                                KeyCode::Char('q') if modifiers.contains(KeyModifiers::CONTROL) => {
                                    should_exit.set(true);
                                }
                                _ => {}
                            }
                        }
                        Pane::List => {
                            match code {
                                KeyCode::Char('q') => {
                                    should_exit.set(true);
                                }
                                KeyCode::Char('/') => {
                                    active_pane.set(Pane::Search);
                                }
                                KeyCode::Char('j') | KeyCode::Down => {
                                    if filtered_len > 0 {
                                        let new_idx =
                                            (selected_index.get() + 1).min(filtered_len - 1);
                                        selected_index.set(new_idx);
                                        // Adjust scroll if needed
                                        if new_idx >= scroll_offset.get() + list_height {
                                            scroll_offset
                                                .set(new_idx.saturating_sub(list_height - 1));
                                        }
                                    }
                                }
                                KeyCode::Char('k') | KeyCode::Up => {
                                    let new_idx = selected_index.get().saturating_sub(1);
                                    selected_index.set(new_idx);
                                    // Adjust scroll if needed
                                    if new_idx < scroll_offset.get() {
                                        scroll_offset.set(new_idx);
                                    }
                                }
                                KeyCode::Char('g') => {
                                    // Go to top
                                    selected_index.set(0);
                                    scroll_offset.set(0);
                                }
                                KeyCode::Char('G') => {
                                    // Go to bottom
                                    if filtered_len > 0 {
                                        let new_idx = filtered_len - 1;
                                        selected_index.set(new_idx);
                                        if new_idx >= list_height {
                                            scroll_offset
                                                .set(new_idx.saturating_sub(list_height - 1));
                                        }
                                    }
                                }
                                KeyCode::Tab => {
                                    active_pane.set(Pane::Detail);
                                }
                                KeyCode::Char('s') => {
                                    // Cycle status for selected ticket
                                    if let Some(ft) = filtered_for_events.get(selected_index.get())
                                        && let Some(id) = &ft.ticket.id
                                        && TicketService::cycle_status(id).is_ok()
                                    {
                                        // Signal to reload tickets
                                        needs_reload.set(true);
                                    }
                                }
                                KeyCode::Char('e') | KeyCode::Enter => {
                                    // Edit selected ticket
                                    if let Some(ft) = filtered_for_events.get(selected_index.get())
                                        && let Some(id) = &ft.ticket.id
                                    {
                                        // Load ticket data and body via service
                                        if let Ok((metadata, body)) =
                                            TicketService::load_for_edit(id)
                                        {
                                            editing_ticket_id.set(id.clone());
                                            editing_ticket.set(metadata);
                                            editing_body.set(body);
                                            is_editing_existing.set(true);
                                        }
                                    }
                                }
                                KeyCode::Char('n') => {
                                    // Create new ticket
                                    is_creating_new.set(true);
                                    is_editing_existing.set(false);
                                    editing_ticket_id.set(String::new());
                                    editing_ticket.set(TicketMetadata::default());
                                    editing_body.set(String::new());
                                }
                                KeyCode::PageDown => {
                                    // Scroll down by half page
                                    let jump = list_height / 2;
                                    let new_idx = (selected_index.get() + jump)
                                        .min(filtered_len.saturating_sub(1));
                                    selected_index.set(new_idx);
                                    if new_idx >= scroll_offset.get() + list_height {
                                        scroll_offset.set(new_idx.saturating_sub(list_height - 1));
                                    }
                                }
                                KeyCode::PageUp => {
                                    // Scroll up by half page
                                    let jump = list_height / 2;
                                    let new_idx = selected_index.get().saturating_sub(jump);
                                    selected_index.set(new_idx);
                                    if new_idx < scroll_offset.get() {
                                        scroll_offset.set(new_idx);
                                    }
                                }
                                _ => {}
                            }
                        }
                        Pane::Detail => match code {
                            KeyCode::Char('q') => {
                                should_exit.set(true);
                            }
                            KeyCode::Tab | KeyCode::Esc => {
                                active_pane.set(Pane::List);
                            }
                            KeyCode::Char('/') => {
                                active_pane.set(Pane::Search);
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                if filtered_len > 0 {
                                    let new_idx = (selected_index.get() + 1).min(filtered_len - 1);
                                    selected_index.set(new_idx);
                                    if new_idx >= scroll_offset.get() + list_height {
                                        scroll_offset.set(new_idx.saturating_sub(list_height - 1));
                                    }
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                let new_idx = selected_index.get().saturating_sub(1);
                                selected_index.set(new_idx);
                                if new_idx < scroll_offset.get() {
                                    scroll_offset.set(new_idx);
                                }
                            }
                            KeyCode::Char('e') | KeyCode::Enter => {
                                // Edit selected ticket
                                if let Some(ft) = filtered_for_events.get(selected_index.get())
                                    && let Some(id) = &ft.ticket.id
                                {
                                    // Load ticket data and body via service
                                    if let Ok((metadata, body)) = TicketService::load_for_edit(id) {
                                        editing_ticket_id.set(id.clone());
                                        editing_ticket.set(metadata);
                                        editing_body.set(body);
                                        is_editing_existing.set(true);
                                    }
                                }
                            }
                            KeyCode::Char('n') => {
                                // Create new ticket
                                is_creating_new.set(true);
                                is_editing_existing.set(false);
                                editing_ticket_id.set(String::new());
                                editing_ticket.set(TicketMetadata::default());
                                editing_body.set(String::new());
                            }
                            _ => {}
                        },
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
            if total_ticket_count == 0 {
                Some(EmptyStateKind::NoTickets)
            } else {
                None
            }
        }
        InitResult::Ok => {
            if total_ticket_count == 0 {
                Some(EmptyStateKind::NoTickets)
            } else if ticket_count == 0 && !query_str.is_empty() {
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
                                    // Left pane: Ticket list
                                    View(
                                        width: 35pct,
                                        height: 100pct,
                                    ) {
                                        TicketList(
                                            tickets: filtered.clone(),
                                            selected_index: selected_index.get(),
                                            scroll_offset: scroll_offset.get(),
                                            has_focus: active_pane.get() == Pane::List && !is_editing,
                                            visible_height: list_height,
                                        )
                                    }

                                    // Right pane: Ticket detail
                                    View(
                                        flex_grow: 1.0,
                                        height: 100pct,
                                    ) {
                                        TicketDetail(
                                            ticket: selected_ticket.clone(),
                                            has_focus: active_pane.get() == Pane::Detail && !is_editing,
                                        )
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
