//! Main remote TUI view component
//!
//! This module provides the main TUI interface for managing local tickets
//! and remote issues with keyboard navigation, list viewing, and detail pane.

use std::collections::HashSet;

use iocraft::prelude::*;

use crate::remote::RemoteIssue;
use crate::ticket::get_all_tickets_from_disk;
use crate::tui::components::{Clickable, InlineSearchBox};
use crate::tui::screen_base::{ScreenLayout, calculate_list_height, should_process_key_event};
use crate::tui::search_orchestrator::{SearchState, compute_filtered_tickets};
use crate::tui::theme::theme;
use crate::types::TicketMetadata;

use super::components::overlays::render_link_mode_banner;
use super::components::{DetailPane, ListPane, ModalOverlays, SelectionBar, TabBar};
use super::confirm_modal::ConfirmDialogState;
use super::error_toast::Toast;
use super::filter::{FilteredLocalTicket, FilteredRemoteIssue, filter_remote_issues};
use super::filter_modal::FilterState;
use super::handlers::{
    HandlerContext,
    async_handlers::{
        FetchResult, create_fetch_handler, create_link_handler, create_push_handler,
        create_search_fetch_handler, create_sync_apply_handler, create_unlink_handler,
    },
    sync_handlers,
    sync_handlers::create_sync_fetch_handler,
};
use super::link_mode::LinkModeState;
use super::shortcuts::{ModalVisibility, compute_shortcuts};
use super::state::{
    DetailScrollData, FilterConfigData, ModalVisibilityData, NavigationData, SearchUiData,
    ViewDisplayData, ViewMode,
};
use super::sync_preview::SyncPreviewState;

/// Props for the RemoteTui component
#[derive(Default, Props)]
pub struct RemoteTuiProps {
    /// Provider type (GitHub or Linear)
    pub provider: Option<String>,
}

/// Main remote TUI component
#[component]
pub fn RemoteTui<'a>(_props: &RemoteTuiProps, mut hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let (width, height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();

    let theme = theme();

    // Grouped state management - related state fields are organized into logical structs

    // Data state: collections of tickets and issues
    let mut local_tickets: State<Vec<TicketMetadata>> =
        hooks.use_state(|| get_all_tickets_from_disk().items);
    let mut remote_issues: State<Vec<RemoteIssue>> = hooks.use_state(Vec::new);

    // Navigation state for list views (selected index, scroll offset, selected IDs)
    let mut local_nav: State<NavigationData> = hooks.use_state(Default::default);
    let mut remote_nav: State<NavigationData> = hooks.use_state(Default::default);

    // View display state (active view, loading, detail visibility, focus, exit flag)
    let mut view_display: State<ViewDisplayData> = hooks.use_state(ViewDisplayData::new);

    // Detail pane scroll state (separate for local and remote)
    let mut detail_scroll: State<DetailScrollData> = hooks.use_state(Default::default);

    // Operation/modal state
    let mut toast: State<Option<Toast>> = hooks.use_state(|| None);
    let mut link_mode: State<Option<LinkModeState>> = hooks.use_state(|| None);
    let mut confirm_dialog: State<Option<ConfirmDialogState>> = hooks.use_state(|| None);
    let mut sync_preview: State<Option<SyncPreviewState>> = hooks.use_state(|| None);
    let mut modal_visibility: State<ModalVisibilityData> = hooks.use_state(Default::default);

    // Last error info (for error detail modal) - stores (type, message)
    let last_error: State<Option<(String, String)>> = hooks.use_state(|| None);

    // Last fetch result for status bar display (stores PaginatedResult with search mode info)
    let last_fetch_result: State<Option<(FetchResult, bool)>> = hooks.use_state(|| None);

    // Search state - search_query is separate for InlineSearchBox compatibility
    let search_query = hooks.use_state(String::new);
    let mut search_ui: State<SearchUiData> = hooks.use_state(Default::default);
    let mut search_state = SearchState::use_state(&mut hooks);

    // Filter and provider configuration
    let mut filter_config: State<FilterConfigData> = hooks.use_state(Default::default);

    // Filter modal state (separate from config since it's a modal overlay)
    let mut filter_state: State<Option<FilterState>> = hooks.use_state(|| None);

    // Cached linked issue IDs (memoization)
    let mut linked_issue_ids_cache: State<(u64, HashSet<String>)> =
        hooks.use_state(|| (0, HashSet::new()));

    // ====================================================================
    // Async handlers - using factory functions to reduce boilerplate
    // ====================================================================

    // Create async handlers via factory functions from async_handlers module
    let fetch_handler = create_fetch_handler(
        &mut hooks,
        &remote_issues,
        &view_display,
        &toast,
        &last_error,
        &last_fetch_result,
    );

    let search_fetch_handler =
        create_search_fetch_handler(&mut hooks, &fetch_handler, &filter_config);

    let push_handler = create_push_handler(
        &mut hooks,
        &local_tickets,
        &fetch_handler,
        &toast,
        &last_error,
        &local_nav,
    );

    let sync_apply_handler = create_sync_apply_handler(
        &mut hooks,
        &local_tickets,
        &fetch_handler,
        &toast,
        &last_error,
    );

    // Create sync action handlers (accept, skip, accept_all, cancel)
    let sync_action_handlers = sync_handlers::create_sync_action_handlers(
        &mut hooks,
        &sync_preview,
        &sync_apply_handler,
        &filter_config,
        &toast,
    );

    let sync_fetch_handler = create_sync_fetch_handler(
        &mut hooks,
        &sync_preview,
        &toast,
        &last_error,
        &sync_action_handlers.accept,
        &sync_action_handlers.skip,
        &sync_action_handlers.accept_all,
        &sync_action_handlers.cancel,
        &filter_config,
    );

    let link_handler = create_link_handler(&mut hooks, &local_tickets, &toast);
    let unlink_handler = create_unlink_handler(&mut hooks, &local_tickets, &local_nav, &toast);

    // ====================================================================
    // Click handlers - using factory functions
    // ====================================================================

    // Tab click handlers
    let tab_local_click_handler = hooks.use_async_handler({
        let view_display = view_display.clone();
        move |()| {
            let mut view_display = view_display.clone();
            async move {
                let mut new_display = view_display.read().clone();
                new_display.set_view(ViewMode::Local);
                view_display.set(new_display);
            }
        }
    });

    let tab_remote_click_handler = hooks.use_async_handler({
        let view_display = view_display.clone();
        move |()| {
            let mut view_display = view_display.clone();
            async move {
                let mut new_display = view_display.read().clone();
                new_display.set_view(ViewMode::Remote);
                view_display.set(new_display);
            }
        }
    });

    // Search focus handler
    let search_click_handler = hooks.use_async_handler({
        let search_ui = search_ui.clone();
        move |()| {
            let mut search_ui = search_ui.clone();
            async move {
                let mut new_ui = search_ui.read().clone();
                new_ui.focused = true;
                search_ui.set(new_ui);
            }
        }
    });

    // List pane click handler
    let list_pane_click_handler = hooks.use_async_handler({
        let search_ui = search_ui.clone();
        let view_display = view_display.clone();
        move |()| {
            let mut search_ui = search_ui.clone();
            let mut view_display = view_display.clone();
            async move {
                let mut new_ui = search_ui.read().clone();
                new_ui.focused = false;
                search_ui.set(new_ui);
                let mut new_display = view_display.read().clone();
                new_display.detail_pane_focused = false;
                view_display.set(new_display);
            }
        }
    });

    // Row click handlers
    let local_row_click_handler = hooks.use_async_handler({
        let local_nav = local_nav.clone();
        move |idx: usize| {
            let mut local_nav = local_nav.clone();
            async move {
                let mut new_nav = local_nav.read().clone();
                new_nav.select_item(idx);
                local_nav.set(new_nav);
            }
        }
    });

    let remote_row_click_handler = hooks.use_async_handler({
        let remote_nav = remote_nav.clone();
        move |idx: usize| {
            let mut remote_nav = remote_nav.clone();
            async move {
                let mut new_nav = remote_nav.read().clone();
                new_nav.select_item(idx);
                remote_nav.set(new_nav);
            }
        }
    });

    // Detail pane click handler
    let detail_pane_click_handler = hooks.use_async_handler({
        let view_display = view_display.clone();
        move |()| {
            let mut view_display = view_display.clone();
            async move {
                let mut new_display = view_display.read().clone();
                new_display.detail_pane_focused = true;
                view_display.set(new_display);
            }
        }
    });

    // Detail scroll handlers
    let detail_scroll_up_handler = hooks.use_async_handler({
        let view_display = view_display.clone();
        let detail_scroll = detail_scroll.clone();
        move |()| {
            let view_display = view_display.clone();
            let mut detail_scroll = detail_scroll.clone();
            async move {
                let current_view = view_display.get().active_view;
                let mut new_scroll = detail_scroll.read().clone();
                new_scroll.scroll_up(current_view, 3);
                detail_scroll.set(new_scroll);
            }
        }
    });

    let detail_scroll_down_handler = hooks.use_async_handler({
        let view_display = view_display.clone();
        let detail_scroll = detail_scroll.clone();
        move |()| {
            let view_display = view_display.clone();
            let mut detail_scroll = detail_scroll.clone();
            async move {
                let current_view = view_display.get().active_view;
                let mut new_scroll = detail_scroll.read().clone();
                new_scroll.scroll_down(current_view, 3);
                detail_scroll.set(new_scroll);
            }
        }
    });

    // Filter modal click handler (no-op since there's only one field)
    let filter_limit_click_handler = hooks.use_async_handler(move |()| async move {});

    // Help scroll handlers
    let help_scroll_up_handler = hooks.use_async_handler({
        let modal_visibility = modal_visibility.clone();
        move |()| {
            let mut modal_visibility = modal_visibility.clone();
            async move {
                let mut visibility = modal_visibility.read().clone();
                visibility.help_scroll = visibility.help_scroll.saturating_sub(3);
                modal_visibility.set(visibility);
            }
        }
    });

    let help_scroll_down_handler = hooks.use_async_handler({
        let modal_visibility = modal_visibility.clone();
        move |()| {
            let mut modal_visibility = modal_visibility.clone();
            async move {
                let mut visibility = modal_visibility.read().clone();
                visibility.help_scroll += 3;
                modal_visibility.set(visibility);
            }
        }
    });

    // ====================================================================
    // Initial data loading and search debounce
    // ====================================================================

    // Track if we've started the initial fetch
    let mut fetch_started = hooks.use_state(|| false);

    // Track last search query for remote search debounce
    let mut last_remote_search_query: State<String> = hooks.use_state(String::new);

    // Trigger initial fetch on startup
    if !fetch_started.get() {
        fetch_started.set(true);
        let filter_config_ref = filter_config.read();
        let current_provider = filter_config_ref.provider;
        let current_query = filter_config_ref.active_filters.clone();
        fetch_handler.clone()((current_provider, current_query));
    }

    // Trigger debounced remote search when query changes in remote view mode
    let view_display_for_search = view_display.read();
    let current_view_for_search = view_display_for_search.active_view;
    drop(view_display_for_search);

    if current_view_for_search == ViewMode::Remote {
        let current_query = search_query.to_string();
        let last_query = last_remote_search_query.to_string();

        if current_query != last_query {
            last_remote_search_query.set(current_query.clone());
            search_fetch_handler.clone()(current_query);
        }
    }

    // Clone handlers for use in event handlers
    let fetch_handler_for_events = fetch_handler.clone();
    let push_handler_for_events = push_handler.clone();
    let sync_fetch_handler_for_events = sync_fetch_handler.clone();
    let sync_apply_handler_for_events = sync_apply_handler.clone();
    let link_handler_for_events = link_handler.clone();
    let unlink_handler_for_events = unlink_handler.clone();

    // ====================================================================
    // Rendering calculations
    // ====================================================================

    // Calculate visible list height for scroll/pagination calculations
    let list_height = calculate_list_height(height, 5);

    // Get current values from grouped state for rendering
    let view_display_ref = view_display.read();
    let local_nav_ref = local_nav.read();
    let remote_nav_ref = remote_nav.read();
    let search_ui_ref = search_ui.read();
    let filter_config_ref = filter_config.read();

    let current_view = view_display_ref.active_view;
    let is_loading = view_display_ref.remote_loading;
    let detail_visible = view_display_ref.show_detail;

    // Read collections for rendering
    let local_tickets_ref = local_tickets.read();
    let remote_issues_ref = remote_issues.read();

    // Get selected IDs from grouped navigation state
    let local_selected_ids = &local_nav_ref.selected_ids;
    let remote_selected_ids = &remote_nav_ref.selected_ids;

    // Compute linked issue IDs (memoized by local tickets length)
    let linked_issue_ids = {
        use crate::tui::remote::operations::extract_issue_id_from_remote_ref;
        let cached_len = linked_issue_ids_cache.read().0;
        let current_len = local_tickets_ref.len() as u64;

        if cached_len == current_len {
            linked_issue_ids_cache.read().1.clone()
        } else {
            let linked: HashSet<String> = local_tickets_ref
                .iter()
                .filter_map(|ticket| ticket.remote.as_ref())
                .filter_map(|remote_ref| extract_issue_id_from_remote_ref(remote_ref))
                .collect();
            linked_issue_ids_cache.set((current_len, linked.clone()));
            linked
        }
    };

    // Compute filtered tickets using SearchState (Enter-triggered search)
    let query_str = search_query.to_string();

    search_state.check_pending(query_str.clone());
    search_state.clear_if_empty(&query_str);

    let filtered_tickets = compute_filtered_tickets(&local_tickets_ref, &search_state, &query_str);

    // Convert FilteredTicket to FilteredLocalTicket for compatibility
    let filtered_local: Vec<FilteredLocalTicket> = filtered_tickets
        .iter()
        .map(|ft| FilteredLocalTicket {
            ticket: ft.ticket.as_ref().clone(),
            score: ft.score,
            title_indices: ft.title_indices.clone(),
        })
        .collect();

    // Remote issues still use client-side filtering (no store search for remote)
    let filtered_remote = filter_remote_issues(&remote_issues_ref, &query_str);

    let local_count = filtered_local.len();
    let remote_count = filtered_remote.len();

    // Counts for footer
    let local_sel_count = local_selected_ids.len();
    let remote_sel_count = remote_selected_ids.len();

    // Cloned data for list rendering
    let local_list: Vec<FilteredLocalTicket> = filtered_local
        .iter()
        .skip(local_nav_ref.scroll_offset)
        .take(list_height)
        .cloned()
        .collect();
    let remote_list: Vec<FilteredRemoteIssue> = filtered_remote
        .iter()
        .skip(remote_nav_ref.scroll_offset)
        .take(list_height)
        .cloned()
        .collect();

    // Clone collection data for the event closure before dropping refs.
    let local_tickets_data = local_tickets_ref.clone();
    let remote_issues_data = remote_issues_ref.clone();

    // Drop refs before creating the event closure (which captures mutable State handles)
    drop(view_display_ref);
    drop(local_nav_ref);
    drop(remote_nav_ref);
    drop(search_ui_ref);
    drop(filter_config_ref);
    drop(local_tickets_ref);
    drop(remote_issues_ref);

    // ====================================================================
    // Keyboard event handling
    // ====================================================================

    hooks.use_terminal_events({
        move |event| match event {
            TerminalEvent::Key(KeyEvent {
                code,
                kind,
                modifiers,
                ..
            }) if should_process_key_event(kind) => {
                // Build the handler context with grouped state references
                use crate::tui::remote::handlers::context::{
                    AsyncHandlers, FilteringState, ModalState, NavigationState, SearchState,
                    ViewData, ViewState,
                };

                let mut ctx = HandlerContext {
                    view_state: ViewState {
                        display: &mut view_display,
                    },
                    view_data: ViewData {
                        local_tickets: &mut local_tickets,
                        remote_issues: &mut remote_issues,
                        local_nav: NavigationState {
                            nav: &mut local_nav,
                        },
                        remote_nav: NavigationState {
                            nav: &mut remote_nav,
                        },
                        local_count,
                        remote_count,
                        list_height,
                        detail_scroll: &mut detail_scroll,
                        local_tickets_data: local_tickets_data.clone(),
                        remote_issues_data: remote_issues_data.clone(),
                    },
                    search: SearchState {
                        ui: &mut search_ui,
                        orchestrator: &mut search_state,
                    },
                    modals: ModalState {
                        toast: &mut toast,
                        link_mode: &mut link_mode,
                        sync_preview: &mut sync_preview,
                        confirm_dialog: &mut confirm_dialog,
                        visibility: &mut modal_visibility,
                        last_error: &last_error,
                    },
                    filters: FilteringState {
                        filter_modal: &mut filter_state,
                        config: &mut filter_config,
                    },
                    handlers: AsyncHandlers {
                        fetch_handler: &fetch_handler_for_events,
                        push_handler: &push_handler_for_events,
                        sync_fetch_handler: &sync_fetch_handler_for_events,
                        sync_apply_handler: &sync_apply_handler_for_events,
                        link_handler: &link_handler_for_events,
                        unlink_handler: &unlink_handler_for_events,
                    },
                };

                // Dispatch to the appropriate handler
                super::handlers::handle_key_event(&mut ctx, code, modifiers);
            }
            _ => {}
        }
    });

    // ====================================================================
    // Prepare data for rendering
    // ====================================================================

    // Exit if requested
    let view_display_ref = view_display.read();
    if view_display_ref.should_exit {
        system.exit();
    }

    // Get selected items from filtered data
    let local_nav_ref = local_nav.read();
    let remote_nav_ref = remote_nav.read();
    let selected_local = filtered_local
        .get(local_nav_ref.selected_index)
        .map(|f| f.ticket.clone());
    let selected_remote = filtered_remote
        .get(remote_nav_ref.selected_index)
        .map(|f| f.issue.clone());

    // Shortcuts for footer - check modals first, then normal mode
    let modal_visibility_ref = modal_visibility.read();
    let search_ui_ref = search_ui.read();
    let shortcuts = compute_shortcuts(
        &ModalVisibility {
            show_help_modal: modal_visibility_ref.show_help,
            show_error_modal: modal_visibility_ref.show_error,
            show_sync_preview: sync_preview.read().is_some(),
            show_confirm_dialog: confirm_dialog.read().is_some(),
            show_link_mode: link_mode.read().is_some(),
            show_filter: filter_state.read().is_some(),
            search_focused: search_ui_ref.focused,
        },
        current_view,
    );

    // Prepare data for components
    let all_local_tickets = local_tickets.read().clone();
    let link_mode_state = link_mode.read().clone();
    let toast_state = toast.read().clone();
    let filter_state_clone = filter_state.read().clone();
    let last_error_clone = last_error.read().clone();
    let sync_preview_state_clone = sync_preview.read().clone();
    let confirm_dialog_state_clone = confirm_dialog.read().clone();

    // Read grouped state values for rendering
    let search_ui_ref = search_ui.read();
    let local_nav_ref = local_nav.read();
    let remote_nav_ref = remote_nav.read();
    let detail_scroll_ref = detail_scroll.read();
    let filter_config_ref = filter_config.read();
    let modal_visibility_ref = modal_visibility.read();
    let last_fetch_result_ref = last_fetch_result.read();

    // Compute status message for remote view
    let status_message = if current_view == ViewMode::Remote {
        last_fetch_result_ref
            .as_ref()
            .and_then(|(result, is_search)| {
                match result {
                    FetchResult::Success(paginated) => {
                        let count = paginated.items.len();
                        let query_str = search_query.to_string();
                        if *is_search {
                            // Search mode
                            if let Some(total) = paginated.total_count {
                                Some(format!(
                                    "Found {total} matches for '{query_str}' ({count} shown)"
                                ))
                            } else {
                                Some(format!("Found {count} matches for '{query_str}'"))
                            }
                        } else {
                            // Browse mode
                            if paginated.has_more {
                                Some(format!("Showing {count} issues (more available)"))
                            } else {
                                Some(format!("Showing {count} issues"))
                            }
                        }
                    }
                    FetchResult::Error(_, _) => None, // Don't show status on error
                }
            })
    } else {
        None
    };

    // ====================================================================
    // Render the UI using sub-components
    // ====================================================================

    element! {
        ScreenLayout(
            width: width,
            height: height,
            header_title: Some("janus remote"),
            header_provider: Some(format!("{}", filter_config_ref.provider)),
            header_extra: Some(vec![element! {
                Text(content: "[?]", color: theme.text_dimmed)
            }.into()]),
            shortcuts: shortcuts,
            toast: toast_state.clone(),
        ) {
            // Tab bar with clickable tabs
            TabBar(
                active_view: current_view,
                filter_query: if query_str.is_empty() { None } else { Some(query_str.clone()) },
                on_local_click: Some(tab_local_click_handler.clone()),
                on_remote_click: Some(tab_remote_click_handler.clone()),
            )

            // Search bar with clickable focus
            Clickable(
                on_click: Some(search_click_handler.clone()),
            ) {
                View(
                    width: 100pct,
                    padding_left: 1,
                    padding_right: 1,
                    height: 1,
                ) {
                        InlineSearchBox(
                        value: Some(search_query),
                        has_focus: search_ui_ref.focused,
                        is_semantic: query_str.starts_with('~'),
                    )
                }
            }

            // Link mode banner
            #(render_link_mode_banner(&link_mode_state))

            // Main content area
            View(
                flex_grow: 1.0,
                width: 100pct,
                flex_direction: FlexDirection::Row,
                overflow: Overflow::Hidden,
            ) {
                // List pane with clickable focus
                Clickable(
                    on_click: Some(list_pane_click_handler.clone()),
                ) {
                    ListPane(
                        view_mode: current_view,
                        is_loading,
                        local_list: local_list.clone(),
                        remote_list: remote_list.clone(),
                        local_count,
                        remote_count,
                        local_scroll_offset: local_nav_ref.scroll_offset,
                        remote_scroll_offset: remote_nav_ref.scroll_offset,
                        local_selected_index: local_nav_ref.selected_index,
                        remote_selected_index: remote_nav_ref.selected_index,
                        local_selected_ids: local_nav_ref.selected_ids.clone(),
                        remote_selected_ids: remote_nav_ref.selected_ids.clone(),
                        all_local_tickets: all_local_tickets.clone(),
                        linked_issue_ids: linked_issue_ids.clone(),
                        on_local_row_click: Some(local_row_click_handler.clone()),
                        on_remote_row_click: Some(remote_row_click_handler.clone()),
                    )
                }

                // Detail pane with clickable focus
                Clickable(
                    on_click: Some(detail_pane_click_handler.clone()),
                ) {
                    DetailPane(
                    view_mode: current_view,
                    selected_remote: selected_remote.clone(),
                    selected_local: selected_local.clone(),
                    visible: detail_visible,
                    remote_scroll_offset: detail_scroll_ref.get_offset(ViewMode::Remote),
                    local_scroll_offset: detail_scroll_ref.get_offset(ViewMode::Local),
                    all_local_tickets: all_local_tickets.clone(),
                    on_scroll_up: Some(detail_scroll_up_handler.clone()),
                    on_scroll_down: Some(detail_scroll_down_handler.clone()),
                    )
                }
            }

            // Selection status bar
            SelectionBar(
                view_mode: current_view,
                local_count: local_sel_count,
                remote_count: remote_sel_count,
                status_message: status_message.clone(),
            )

            // Modal overlays
            ModalOverlays(
                filter_state: filter_state_clone,
                on_filter_limit_click: Some(filter_limit_click_handler.clone()),
                show_help_modal: modal_visibility_ref.show_help,
                help_modal_scroll: modal_visibility_ref.help_scroll,
                on_help_scroll_up: Some(help_scroll_up_handler.clone()),
                on_help_scroll_down: Some(help_scroll_down_handler.clone()),
                show_error_modal: modal_visibility_ref.show_error,
                last_error: last_error_clone,
                sync_preview_state: sync_preview_state_clone,
                confirm_dialog_state: confirm_dialog_state_clone,
            )
        }
    }
}
