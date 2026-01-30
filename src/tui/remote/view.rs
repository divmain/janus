//! Main remote TUI view component
//!
//! This module provides the main TUI interface for managing local tickets
//! and remote issues with keyboard navigation, list viewing, and detail pane.

// Allow clone on Copy types - used intentionally in async closures for clarity
#![allow(clippy::clone_on_copy)]
#![allow(clippy::redundant_closure)]

use std::collections::HashSet;

use futures::stream::{self, StreamExt};
use iocraft::prelude::*;

use crate::remote::config::Platform;
use crate::remote::{RemoteIssue, RemoteProvider, RemoteQuery};
use crate::ticket::get_all_tickets_from_disk;
use crate::tui::components::InlineSearchBox;
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
use super::handlers::{self, HandlerContext};
use super::link_mode::LinkModeState;
use super::shortcuts::{ModalVisibility, compute_shortcuts};
use super::state::ViewMode;
use super::sync_preview::SyncPreviewState;

/// Result from async fetch operation
#[derive(Clone)]
enum FetchResult {
    Success(Vec<RemoteIssue>),
    Error(String, String), // (error_type, error_message)
}

/// Fetch remote issues from the given provider with optional query filters
async fn fetch_remote_issues_with_query(platform: Platform, query: RemoteQuery) -> FetchResult {
    let config = match crate::remote::config::Config::load() {
        Ok(c) => c,
        Err(e) => {
            return FetchResult::Error("ConfigError".to_string(), e.to_string());
        }
    };

    let result = match platform {
        Platform::GitHub => match crate::remote::github::GitHubProvider::from_config(&config) {
            Ok(provider) => provider.list_issues(&query).await,
            Err(e) => Err(e),
        },
        Platform::Linear => match crate::remote::linear::LinearProvider::from_config(&config) {
            Ok(provider) => provider.list_issues(&query).await,
            Err(e) => Err(e),
        },
    };

    match result {
        Ok(issues) => FetchResult::Success(issues),
        Err(e) => FetchResult::Error("FetchError".to_string(), e.to_string()),
    }
}

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

    // State management - using individual State values like view.rs
    let mut active_view = hooks.use_state(|| ViewMode::Local);
    let mut local_tickets: State<Vec<TicketMetadata>> = hooks.use_state(|| {
        get_all_tickets_from_disk().unwrap_or_else(|e| {
            eprintln!("Error: failed to load tickets: {}", e);
            vec![]
        })
    });
    let mut remote_issues: State<Vec<RemoteIssue>> = hooks.use_state(Vec::new);

    let mut local_selected_index = hooks.use_state(|| 0usize);
    let mut remote_selected_index = hooks.use_state(|| 0usize);
    let mut local_scroll_offset = hooks.use_state(|| 0usize);
    let mut remote_scroll_offset = hooks.use_state(|| 0usize);

    let mut local_selected_ids: State<HashSet<String>> = hooks.use_state(HashSet::new);
    let mut remote_selected_ids: State<HashSet<String>> = hooks.use_state(HashSet::new);

    let mut remote_loading = hooks.use_state(|| true);
    let mut show_detail = hooks.use_state(|| true);
    let mut should_exit = hooks.use_state(|| false);

    let mut detail_pane_focused = hooks.use_state(|| false);
    let mut local_detail_scroll_offset = hooks.use_state(|| 0usize);
    let mut remote_detail_scroll_offset = hooks.use_state(|| 0usize);

    // Operation state
    let mut toast: State<Option<Toast>> = hooks.use_state(|| None);
    let mut link_mode: State<Option<LinkModeState>> = hooks.use_state(|| None);
    let mut confirm_dialog: State<Option<ConfirmDialogState>> = hooks.use_state(|| None);
    let mut sync_preview: State<Option<SyncPreviewState>> = hooks.use_state(|| None);
    let mut show_help_modal = hooks.use_state(|| false);
    let mut help_modal_scroll = hooks.use_state(|| 0usize);
    let mut show_error_modal = hooks.use_state(|| false);

    // Last error info (for error detail modal) - stores (type, message)
    let last_error: State<Option<(String, String)>> = hooks.use_state(|| None);

    // Search state - search is executed on Enter, not while typing
    let mut search_query = hooks.use_state(String::new);
    let mut search_state = SearchState::use_state(&mut hooks);
    let mut search_focused = hooks.use_state(|| false);

    // Provider state (GitHub or Linear)
    let mut provider = hooks.use_state(|| Platform::GitHub);

    // Filter state
    let mut filter_state: State<Option<FilterState>> = hooks.use_state(|| None);
    let mut active_filters = hooks.use_state(|| RemoteQuery::new());

    // Cached linked issue IDs (memoization)
    let mut linked_issue_ids_cache: State<(u64, HashSet<String>)> =
        hooks.use_state(|| (0, HashSet::new()));

    // Async fetch handler for refreshing remote issues
    let fetch_handler: Handler<(Platform, RemoteQuery)> = hooks.use_async_handler({
        let remote_issues_setter = remote_issues.clone();
        let remote_loading_setter = remote_loading.clone();
        let toast_setter = toast.clone();
        let last_error_setter = last_error.clone();

        move |(platform, query): (Platform, RemoteQuery)| {
            let mut remote_issues_setter = remote_issues_setter.clone();
            let mut remote_loading_setter = remote_loading_setter.clone();
            let mut toast_setter = toast_setter.clone();
            let mut last_error_setter = last_error_setter.clone();

            async move {
                let result = fetch_remote_issues_with_query(platform, query).await;
                match result {
                    FetchResult::Success(issues) => {
                        remote_issues_setter.set(issues);
                    }
                    FetchResult::Error(err_type, err_msg) => {
                        last_error_setter.set(Some((err_type, err_msg.clone())));
                        toast_setter.set(Some(Toast::error(format!(
                            "Failed to fetch remote issues: {}",
                            err_msg
                        ))));
                    }
                }
                remote_loading_setter.set(false);
            }
        }
    });

    // Track if we've started the initial fetch
    let mut fetch_started = hooks.use_state(|| false);

    // Trigger initial fetch on startup
    if !fetch_started.get() {
        fetch_started.set(true);
        let current_provider = provider.get();
        let current_query = active_filters.read().clone();
        fetch_handler.clone()((current_provider, current_query));
    }

    // Clone fetch_handler for use in event handlers
    let fetch_handler_for_events = fetch_handler.clone();

    // Async push handler for pushing local tickets to remote
    let push_handler: Handler<(Vec<String>, Platform, RemoteQuery)> = hooks.use_async_handler({
        let local_tickets_setter = local_tickets.clone();
        let fetch_handler = fetch_handler.clone();
        let toast_setter = toast.clone();
        let last_error_setter = last_error.clone();
        let local_selected_ids_setter = local_selected_ids.clone();

        move |(ticket_ids, platform, query): (Vec<String>, Platform, RemoteQuery)| {
            let mut local_tickets_setter = local_tickets_setter.clone();
            let fetch_handler = fetch_handler.clone();
            let mut toast_setter = toast_setter.clone();
            let mut last_error_setter = last_error_setter.clone();
            let mut local_selected_ids_setter = local_selected_ids_setter.clone();

            async move {
                let (successes, errors) =
                    super::operations::push_tickets_to_remote(&ticket_ids, platform).await;

                if !errors.is_empty() {
                    let error_msgs: Vec<String> = errors
                        .iter()
                        .map(|e| format!("{}: {}", e.ticket_id(), e.error_message()))
                        .collect();
                    last_error_setter.set(Some(("Push Errors".to_string(), error_msgs.join("\n"))));
                }

                if successes.is_empty() && !errors.is_empty() {
                    toast_setter.set(Some(Toast::error(format!(
                        "Push failed for {}: {}",
                        errors[0].ticket_id(),
                        errors[0].error_message()
                    ))));
                } else if errors.is_empty() {
                    // Show more detail for successful pushes
                    let msg = if successes.len() == 1 {
                        format!(
                            "Pushed {} -> {}",
                            successes[0].ticket_id, successes[0].remote_ref
                        )
                    } else {
                        let ids: Vec<&str> =
                            successes.iter().map(|s| s.ticket_id.as_str()).collect();
                        format!("Pushed {} tickets: {}", successes.len(), ids.join(", "))
                    };
                    toast_setter.set(Some(Toast::info(msg)));
                } else {
                    // Mixed results - show what succeeded and what failed
                    let success_ids: Vec<&str> =
                        successes.iter().map(|s| s.ticket_id.as_str()).collect();
                    toast_setter.set(Some(Toast::warning(format!(
                        "Pushed {}, failed: {}",
                        success_ids.join(", "),
                        errors
                            .iter()
                            .map(|e| e.ticket_id())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))));
                }

                // Refresh local tickets to show updated remote links
                local_tickets_setter.set(get_all_tickets_from_disk().unwrap_or_else(|e| {
                    eprintln!("Error: failed to load tickets: {}", e);
                    vec![]
                }));

                // Clear selection
                local_selected_ids_setter.set(HashSet::new());

                // Refresh remote issues to show new issues
                fetch_handler((platform, query));
            }
        }
    });

    let push_handler_for_events = push_handler.clone();

    // Async sync handler for fetching remote data and building changes
    let sync_fetch_handler: Handler<(Vec<String>, Platform)> = hooks.use_async_handler({
        let sync_preview_setter = sync_preview.clone();
        let toast_setter = toast.clone();
        let last_error_setter = last_error.clone();

        move |(ticket_ids, platform): (Vec<String>, Platform)| {
            let mut sync_preview_setter = sync_preview_setter.clone();
            let mut toast_setter = toast_setter.clone();
            let mut last_error_setter = last_error_setter.clone();

            async move {
                use super::sync_preview::SyncChangeWithContext;

                let owned_ticket_ids: Vec<String> = ticket_ids.to_vec();
                let mut all_changes: Vec<SyncChangeWithContext> = Vec::new();
                let mut error_messages: Vec<String> = Vec::new();

                let results: Vec<_> = stream::iter(owned_ticket_ids)
                    .map(|ticket_id| async {
                        let result =
                            super::operations::fetch_remote_issue_for_ticket(&ticket_id, platform)
                                .await;
                        (ticket_id, result)
                    })
                    .buffer_unordered(5)
                    .collect()
                    .await;

                for (ticket_id, fetch_result) in results {
                    match fetch_result {
                        Ok((metadata, issue)) => {
                            match super::operations::build_sync_changes(&metadata, &issue) {
                                Ok(changes) => {
                                    let remote_ref = metadata.remote.clone().unwrap_or_default();

                                    for change in changes {
                                        all_changes.push(SyncChangeWithContext {
                                            ticket_id: ticket_id.clone(),
                                            remote_ref: remote_ref.clone(),
                                            change,
                                            decision: None,
                                        });
                                    }
                                }
                                Err(e) => {
                                    error_messages.push(format!(
                                        "Failed to build sync changes for {}: {}",
                                        ticket_id, e
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            error_messages
                                .push(format!("Failed to fetch remote for {}: {}", ticket_id, e));
                        }
                    }
                }

                if !error_messages.is_empty() {
                    last_error_setter
                        .set(Some(("SyncError".to_string(), error_messages.join("\n"))));
                }

                if all_changes.is_empty() {
                    toast_setter.set(Some(Toast::info(
                        "No differences found between local and remote",
                    )));
                } else {
                    toast_setter.set(Some(Toast::info(format!(
                        "Found {} change(s) to review",
                        all_changes.len()
                    ))));
                    sync_preview_setter.set(Some(super::sync_preview::SyncPreviewState::new(
                        all_changes,
                    )));
                }
            }
        }
    });

    let sync_fetch_handler_for_events = sync_fetch_handler.clone();

    // Handler to apply accepted sync changes
    let sync_apply_handler: Handler<(
        super::sync_preview::SyncPreviewState,
        Platform,
        RemoteQuery,
    )> = hooks.use_async_handler({
        let local_tickets_setter = local_tickets.clone();
        let fetch_handler = fetch_handler.clone();
        let toast_setter = toast.clone();
        let last_error_setter = last_error.clone();

        move |(state, platform, query): (
            super::sync_preview::SyncPreviewState,
            Platform,
            RemoteQuery,
        )| {
            let mut local_tickets_setter = local_tickets_setter.clone();
            let fetch_handler = fetch_handler.clone();
            let mut toast_setter = toast_setter.clone();
            let mut last_error_setter = last_error_setter.clone();

            async move {
                let accepted = state.accepted_changes();
                let mut applied = 0;
                let mut errors = Vec::new();

                for change_ctx in accepted {
                    let result = match change_ctx.change.direction {
                        super::sync_preview::SyncDirection::RemoteToLocal => {
                            super::operations::apply_sync_change_to_local(
                                &change_ctx.ticket_id,
                                &change_ctx.change,
                            )
                            .await
                        }
                        super::sync_preview::SyncDirection::LocalToRemote => {
                            super::operations::apply_sync_change_to_remote(
                                &change_ctx.remote_ref,
                                &change_ctx.change,
                                platform,
                            )
                            .await
                        }
                    };

                    match result {
                        Ok(()) => applied += 1,
                        Err(e) => errors.push(e.to_string()),
                    }
                }

                if !errors.is_empty() {
                    last_error_setter.set(Some(("SyncApplyError".to_string(), errors.join("\n"))));
                }

                if applied > 0 {
                    toast_setter.set(Some(Toast::info(format!("Applied {} change(s)", applied))));
                    local_tickets_setter.set(get_all_tickets_from_disk().unwrap_or_else(|e| {
                        eprintln!("Error: failed to load tickets: {}", e);
                        vec![]
                    }));
                    fetch_handler((platform, query));
                } else if !errors.is_empty() {
                    toast_setter.set(Some(Toast::error("Failed to apply changes")));
                }
            }
        }
    });

    let sync_apply_handler_for_events = sync_apply_handler.clone();

    // Async link handler for linking a local ticket to a remote issue
    let link_handler: Handler<super::link_mode::LinkSource> = hooks.use_async_handler({
        let local_tickets_setter = local_tickets.clone();
        let toast_setter = toast.clone();

        move |source: super::link_mode::LinkSource| {
            let mut local_tickets_setter = local_tickets_setter.clone();
            let mut toast_setter = toast_setter.clone();

            async move {
                match super::operations::link_ticket_to_issue(
                    &source.ticket_id,
                    &source.remote_issue,
                )
                .await
                {
                    Ok(()) => {
                        toast_setter.set(Some(Toast::info(format!(
                            "Linked {} to {}",
                            source.ticket_id, source.remote_issue.id
                        ))));
                        local_tickets_setter.set(get_all_tickets_from_disk().unwrap_or_else(|e| {
                            eprintln!("Error: failed to load tickets: {}", e);
                            vec![]
                        }));
                    }
                    Err(e) => {
                        toast_setter.set(Some(Toast::error(format!("Link failed: {}", e))));
                    }
                }
            }
        }
    });

    let link_handler_for_events = link_handler.clone();

    // Async unlink handler for unlinking local tickets from remote issues
    let unlink_handler: Handler<Vec<String>> = hooks.use_async_handler({
        let local_tickets_setter = local_tickets.clone();
        let local_selected_ids_setter = local_selected_ids.clone();
        let toast_setter = toast.clone();

        move |ticket_ids: Vec<String>| {
            let mut local_tickets_setter = local_tickets_setter.clone();
            let mut local_selected_ids_setter = local_selected_ids_setter.clone();
            let mut toast_setter = toast_setter.clone();

            async move {
                let mut unlinked = 0;
                let mut errors: Vec<(String, String)> = Vec::new();

                for id in &ticket_ids {
                    match super::operations::unlink_ticket(id).await {
                        Ok(()) => unlinked += 1,
                        Err(e) => errors.push((id.clone(), e.to_string())),
                    }
                }

                // Always refresh and clear selection if any operations succeeded
                if unlinked > 0 {
                    local_tickets_setter.set(get_all_tickets_from_disk().unwrap_or_else(|e| {
                        eprintln!("Error: failed to load tickets: {}", e);
                        vec![]
                    }));
                    local_selected_ids_setter.set(HashSet::new());
                }

                // Report results
                if errors.is_empty() {
                    toast_setter.set(Some(Toast::info(format!(
                        "Unlinked {} ticket(s)",
                        unlinked
                    ))));
                } else if unlinked > 0 {
                    // Partial success
                    toast_setter.set(Some(Toast::warning(format!(
                        "Unlinked {}, failed {} (see logs)",
                        unlinked,
                        errors.len()
                    ))));
                    // Log detailed errors
                    for (id, err) in errors {
                        eprintln!("Failed to unlink {}: {}", id, err);
                    }
                } else {
                    // Total failure
                    if errors.len() == 1 {
                        toast_setter.set(Some(Toast::error(format!(
                            "Failed to unlink: {}",
                            errors[0].1
                        ))));
                    } else {
                        toast_setter.set(Some(Toast::error(format!(
                            "Failed to unlink {} ticket(s) (see logs)",
                            errors.len()
                        ))));
                        // Log detailed errors
                        for (id, err) in errors {
                            eprintln!("Failed to unlink {}: {}", id, err);
                        }
                    }
                }
            }
        }
    });

    let unlink_handler_for_events = unlink_handler.clone();

    // Calculate visible list height for scroll/pagination calculations
    // This is NOT for layout (handled by flexbox) but for determining how many
    // items fit in the visible area for keyboard navigation and scroll offset.
    // Additional elements: tabs(1) + search(1) + link_banner(1) + selection_bar(1) + borders(1) = 5
    let list_height = calculate_list_height(height, 5);

    // Get current values for rendering
    let current_view = active_view.get();
    let is_loading = remote_loading.get();
    let detail_visible = show_detail.get();

    // Read collections for rendering
    let local_tickets_ref = local_tickets.read();
    let remote_issues_ref = remote_issues.read();
    let local_selected_ref = local_selected_ids.read();
    let remote_selected_ref = remote_selected_ids.read();

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
    let filtered_local: Vec<super::filter::FilteredLocalTicket> = filtered_tickets
        .iter()
        .map(|ft| super::filter::FilteredLocalTicket {
            ticket: ft.ticket.as_ref().clone(),
            score: ft.score,
            title_indices: ft.title_indices.clone(),
        })
        .collect();

    // Remote issues still use client-side filtering (no cache search for remote)
    let filtered_remote = filter_remote_issues(&remote_issues_ref, &query_str);

    let local_count = filtered_local.len();
    let remote_count = filtered_remote.len();

    // Note: We don't pre-compute selected_local/selected_remote here since we need them
    // after the event closure, which would move them. We compute them at render time.

    // Counts for footer
    let local_sel_count = local_selected_ref.len();
    let remote_sel_count = remote_selected_ref.len();

    // Cloned data for list rendering
    let local_list: Vec<FilteredLocalTicket> = filtered_local
        .iter()
        .skip(local_scroll_offset.get())
        .take(list_height)
        .cloned()
        .collect();
    let remote_list: Vec<FilteredRemoteIssue> = filtered_remote
        .iter()
        .skip(remote_scroll_offset.get())
        .take(list_height)
        .cloned()
        .collect();

    // Clone collection data for the event closure before dropping refs.
    // This simplifies the pattern: instead of complex drop ordering, we clone
    // what the closure needs and pass it through the context.
    let local_tickets_data = local_tickets_ref.clone();
    let remote_issues_data = remote_issues_ref.clone();

    // Drop refs before creating the event closure (which captures mutable State handles)
    drop(local_tickets_ref);
    drop(remote_issues_ref);
    drop(local_selected_ref);
    drop(remote_selected_ref);

    // Keyboard event handling - dispatched to separate handler modules
    hooks.use_terminal_events({
        move |event| match event {
            TerminalEvent::Key(KeyEvent {
                code,
                kind,
                modifiers,
                ..
            }) if should_process_key_event(kind) => {
                // Build the handler context with grouped state references
                use handlers::context::{
                    AsyncHandlers, FilteringState, ModalState, NavigationState, RemoteState,
                    SearchState, ViewData, ViewState,
                };

                let mut ctx = HandlerContext {
                    view_state: ViewState {
                        active_view: &mut active_view,
                        show_detail: &mut show_detail,
                        should_exit: &mut should_exit,
                        detail_pane_focused: &mut detail_pane_focused,
                    },
                    view_data: ViewData {
                        local_tickets: &mut local_tickets,
                        remote_issues: &mut remote_issues,
                        local_nav: NavigationState {
                            selected_index: &mut local_selected_index,
                            scroll_offset: &mut local_scroll_offset,
                            selected_ids: &mut local_selected_ids,
                        },
                        remote_nav: NavigationState {
                            selected_index: &mut remote_selected_index,
                            scroll_offset: &mut remote_scroll_offset,
                            selected_ids: &mut remote_selected_ids,
                        },
                        local_count,
                        remote_count,
                        list_height,
                        local_detail_scroll_offset: &mut local_detail_scroll_offset,
                        remote_detail_scroll_offset: &mut remote_detail_scroll_offset,
                        local_tickets_data: local_tickets_data.clone(),
                        remote_issues_data: remote_issues_data.clone(),
                    },
                    search: SearchState {
                        query: &mut search_query,
                        focused: &mut search_focused,
                        orchestrator: &mut search_state,
                    },
                    modals: ModalState {
                        toast: &mut toast,
                        link_mode: &mut link_mode,
                        sync_preview: &mut sync_preview,
                        confirm_dialog: &mut confirm_dialog,
                        show_help_modal: &mut show_help_modal,
                        help_modal_scroll: &mut help_modal_scroll,
                        show_error_modal: &mut show_error_modal,
                        last_error: &last_error,
                    },
                    filters: FilteringState {
                        filter_modal: &mut filter_state,
                        active_filters: &mut active_filters,
                        provider: &mut provider,
                    },
                    remote: RemoteState {
                        loading: &mut remote_loading,
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
                handlers::handle_key_event(&mut ctx, code, modifiers);
            }
            _ => {}
        }
    });

    // Exit if requested
    if should_exit.get() {
        system.exit();
    }

    // Get selected items from filtered data
    let selected_local = filtered_local
        .get(local_selected_index.get())
        .map(|f| f.ticket.clone());
    let selected_remote = filtered_remote
        .get(remote_selected_index.get())
        .map(|f| f.issue.clone());

    // Shortcuts for footer - check modals first, then normal mode
    let shortcuts = compute_shortcuts(
        &ModalVisibility {
            show_help_modal: show_help_modal.get(),
            show_error_modal: show_error_modal.get(),
            show_sync_preview: sync_preview.read().is_some(),
            show_confirm_dialog: confirm_dialog.read().is_some(),
            show_link_mode: link_mode.read().is_some(),
            show_filter: filter_state.read().is_some(),
            search_focused: search_focused.get(),
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

    // Render the UI using sub-components
    element! {
        ScreenLayout(
            width: width,
            height: height,
            header_title: Some("janus remote"),
            header_provider: Some(format!("{}", provider.get())),
            header_extra: Some(vec![element! {
                Text(content: "[?]", color: theme.text_dimmed)
            }.into()]),
            shortcuts: shortcuts,
            toast: toast_state.clone(),
        ) {
            // Tab bar
            TabBar(
                active_view: current_view,
                filter_query: if query_str.is_empty() { None } else { Some(query_str.clone()) },
            )

            // Search bar
            View(
                width: 100pct,
                padding_left: 1,
                padding_right: 1,
                height: 1,
            ) {
                InlineSearchBox(
                    value: Some(search_query),
                    has_focus: search_focused.get(),
                    is_semantic: query_str.starts_with('~'),
                )
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
                // List pane
                ListPane(
                    view_mode: current_view,
                    is_loading,
                    local_list: local_list.clone(),
                    remote_list: remote_list.clone(),
                    local_count,
                    remote_count,
                    local_scroll_offset: local_scroll_offset.get(),
                    remote_scroll_offset: remote_scroll_offset.get(),
                    local_selected_index: local_selected_index.get(),
                    remote_selected_index: remote_selected_index.get(),
                    local_selected_ids: local_selected_ids.read().clone(),
                    remote_selected_ids: remote_selected_ids.read().clone(),
                    all_local_tickets: all_local_tickets.clone(),
                    linked_issue_ids: linked_issue_ids.clone(),
                )

                // Detail pane
                DetailPane(
                    view_mode: current_view,
                    selected_remote: selected_remote.clone(),
                    selected_local: selected_local.clone(),
                    visible: detail_visible,
                    remote_scroll_offset: remote_detail_scroll_offset.get(),
                    local_scroll_offset: local_detail_scroll_offset.get(),
                    all_local_tickets: all_local_tickets.clone(),
                )
            }

            // Selection status bar
            SelectionBar(
                view_mode: current_view,
                local_count: local_sel_count,
                remote_count: remote_sel_count,
            )

            // Modal overlays
            ModalOverlays(
                filter_state: filter_state_clone,
                show_help_modal: show_help_modal.get(),
                help_modal_scroll: help_modal_scroll.get(),
                show_error_modal: show_error_modal.get(),
                last_error: last_error_clone,
                sync_preview_state: sync_preview_state_clone,
                confirm_dialog_state: confirm_dialog_state_clone,
            )
        }
    }
}
