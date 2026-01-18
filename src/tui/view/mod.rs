//! Issue browser view (`janus view`)
//!
//! Provides an interactive TUI for browsing and managing tickets with
//! fuzzy search, keyboard navigation, and inline detail viewing.

pub mod handlers;

use iocraft::prelude::*;

use crate::tui::components::{
    EmptyState, EmptyStateKind, Footer, Header, SearchBox, TicketDetail, TicketList,
    browser_shortcuts, edit_shortcuts, empty_shortcuts, search_shortcuts,
};
use crate::tui::edit::{EditForm, EditResult};
use crate::tui::edit_state::EditFormState;
use crate::tui::search::{FilteredTicket, filter_tickets};
use crate::tui::state::{InitResult, Pane, TuiState};
use crate::tui::theme::theme;
use crate::types::TicketMetadata;

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
        hooks.use_state(|| TuiState::new_sync().repository.tickets);
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
