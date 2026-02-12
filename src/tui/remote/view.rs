//! Main remote TUI view component
//!
//! This module provides the main TUI interface for managing local tickets
//! and remote issues with keyboard navigation, list viewing, and detail pane.

// Allow clone on Copy types - used intentionally in async closures for clarity
#![allow(clippy::clone_on_copy)]
#![allow(clippy::redundant_closure)]

use std::collections::HashSet;

/// Debounce delay for remote search input in milliseconds
/// This prevents excessive API calls while typing
const REMOTE_SEARCH_DEBOUNCE_MS: u64 = 500;

/// Macro to reduce boilerplate in async handlers by consolidating State cloning.
///
/// This macro simplifies the repetitive pattern where each async handler needs
/// to clone State values twice (outside closure and inside closure for async block).
///
/// Usage:
///   clone_states!(var1, var2, var3);  
///   // expands to: let var1 = var1.clone(); let var2 = var2.clone(); ...
macro_rules! clone_states {
    ($($var:ident),+ $(,)?) => {
        $(let $var = $var.clone();)+
    };
}

use futures::stream::{self, StreamExt};
use iocraft::prelude::*;

use crate::remote::Platform;
use crate::remote::{PaginatedResult, RemoteIssue, RemoteProvider, RemoteQuery};
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
use super::handlers::{self, HandlerContext};
use super::link_mode::LinkModeState;
use super::shortcuts::{ModalVisibility, compute_shortcuts};
use super::state::ViewMode;
use super::sync_preview::SyncPreviewState;

/// Result from async fetch operation with pagination metadata
#[derive(Clone)]
enum FetchResult {
    Success(PaginatedResult<RemoteIssue>),
    Error(String, String), // (error_type, error_message)
}

/// Fetch remote issues from the given provider with optional query filters
///
/// This operation supports dual-mode fetching:
/// - Browse mode: When query.search_text is None, calls browse_issues() to fetch
///   up to max_pages (default 5 = 500 issues) for browsing recent issues.
/// - Search mode: When query.search_text is Some, calls search_remote() to perform
///   server-side text search across all issues.
///
/// The operation is wrapped with a timeout from config (default 30 seconds)
/// to prevent indefinite hanging if the remote provider is unresponsive.
async fn fetch_remote_issues_with_query(platform: Platform, query: RemoteQuery) -> FetchResult {
    let config = match crate::config::Config::load() {
        Ok(c) => c,
        Err(e) => {
            return FetchResult::Error("ConfigError".to_string(), e.to_string());
        }
    };

    let timeout = config.remote_timeout();
    let is_search_mode = query.search_text.is_some();

    let fetch_operation = async {
        match platform {
            Platform::GitHub => match crate::remote::github::GitHubProvider::from_config(&config) {
                Ok(provider) => {
                    if is_search_mode {
                        let text = query.search_text.as_ref().unwrap();
                        provider.search_remote(text, &query).await
                    } else {
                        provider.browse_issues(&query).await
                    }
                }
                Err(e) => Err(e),
            },
            Platform::Linear => match crate::remote::linear::LinearProvider::from_config(&config) {
                Ok(provider) => {
                    if is_search_mode {
                        let text = query.search_text.as_ref().unwrap();
                        provider.search_remote(text, &query).await
                    } else {
                        provider.browse_issues(&query).await
                    }
                }
                Err(e) => Err(e),
            },
        }
    };

    let result = match tokio::time::timeout(timeout, fetch_operation).await {
        Ok(result) => result,
        Err(_) => {
            return FetchResult::Error(
                "TimeoutError".to_string(),
                format!(
                    "Remote operation timed out after {} seconds",
                    timeout.as_secs()
                ),
            );
        }
    };

    match result {
        Ok(paginated) => FetchResult::Success(paginated),
        Err(e) => {
            // Check if it's a timeout error from the inner retry mechanism
            let error_msg = if let crate::error::JanusError::RemoteTimeout { seconds } = &e {
                format!("Remote operation timed out after {seconds} seconds")
            } else {
                e.to_string()
            };
            FetchResult::Error("FetchError".to_string(), error_msg)
        }
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

    // Grouped state management - related state fields are organized into logical structs
    // This reduces the number of individual State values and makes the code more maintainable

    // Data state: collections of tickets and issues
    let mut local_tickets: State<Vec<TicketMetadata>> =
        hooks.use_state(|| get_all_tickets_from_disk().items);
    let mut remote_issues: State<Vec<RemoteIssue>> = hooks.use_state(Vec::new);

    // Navigation state for list views (selected index, scroll offset, selected IDs)
    let mut local_nav: State<super::state::NavigationData> = hooks.use_state(Default::default);
    let mut remote_nav: State<super::state::NavigationData> = hooks.use_state(Default::default);

    // View display state (active view, loading, detail visibility, focus, exit flag)
    let mut view_display: State<super::state::ViewDisplayData> =
        hooks.use_state(super::state::ViewDisplayData::new);

    // Detail pane scroll state (separate for local and remote)
    let mut detail_scroll: State<super::state::DetailScrollData> =
        hooks.use_state(Default::default);

    // Operation/modal state
    let mut toast: State<Option<Toast>> = hooks.use_state(|| None);
    let mut link_mode: State<Option<LinkModeState>> = hooks.use_state(|| None);
    let mut confirm_dialog: State<Option<ConfirmDialogState>> = hooks.use_state(|| None);
    let mut sync_preview: State<Option<SyncPreviewState>> = hooks.use_state(|| None);
    let mut modal_visibility: State<super::state::ModalVisibilityData> =
        hooks.use_state(Default::default);

    // Last error info (for error detail modal) - stores (type, message)
    let last_error: State<Option<(String, String)>> = hooks.use_state(|| None);

    // Last fetch result for status bar display (stores PaginatedResult with search mode info)
    let last_fetch_result: State<Option<(FetchResult, bool)>> = hooks.use_state(|| None);

    // Search state - search_query is separate for InlineSearchBox compatibility
    let search_query = hooks.use_state(String::new);
    let mut search_ui: State<super::state::SearchUiData> = hooks.use_state(Default::default);
    let mut search_state = SearchState::use_state(&mut hooks);

    // Filter and provider configuration
    let mut filter_config: State<super::state::FilterConfigData> =
        hooks.use_state(Default::default);

    // Filter modal state (separate from config since it's a modal overlay)
    let mut filter_state: State<Option<FilterState>> = hooks.use_state(|| None);

    // Cached linked issue IDs (memoization)
    let mut linked_issue_ids_cache: State<(u64, HashSet<String>)> =
        hooks.use_state(|| (0, HashSet::new()));

    // Async fetch handler for refreshing remote issues
    let fetch_handler: Handler<(Platform, RemoteQuery)> = hooks.use_async_handler({
        clone_states!(
            remote_issues,
            view_display,
            toast,
            last_error,
            last_fetch_result
        );
        move |(platform, query): (Platform, RemoteQuery)| {
            let mut remote_issues = remote_issues.clone();
            let mut view_display = view_display.clone();
            let mut toast = toast.clone();
            let mut last_error = last_error.clone();
            let mut last_fetch_result = last_fetch_result.clone();
            async move {
                let is_search = query.search_text.is_some();
                let result = fetch_remote_issues_with_query(platform, query).await;
                match result {
                    FetchResult::Success(paginated) => {
                        let items = paginated.items.clone();
                        remote_issues.set(items.clone());

                        // Show info toast about results
                        let msg = if is_search {
                            format!("Found {} matches", items.len())
                        } else if paginated.has_more {
                            format!("Loaded {} issues (more available)", items.len())
                        } else {
                            format!("Loaded {} issues", items.len())
                        };
                        toast.set(Some(Toast::info(msg)));

                        // Store result for status bar display
                        last_fetch_result.set(Some((FetchResult::Success(paginated), is_search)));
                    }
                    FetchResult::Error(err_type, err_msg) => {
                        last_error.set(Some((err_type.clone(), err_msg.clone())));
                        toast.set(Some(Toast::error(format!(
                            "Failed to fetch remote issues: {err_msg}"
                        ))));

                        // Store error result for status bar display
                        last_fetch_result
                            .set(Some((FetchResult::Error(err_type, err_msg), is_search)));
                    }
                }
                // Update loading state within view_display struct
                let mut new_display = view_display.read().clone();
                new_display.remote_loading = false;
                view_display.set(new_display);
            }
        }
    });

    // Search orchestrator with debounce for remote search
    // This triggers a debounced fetch when search text changes
    let search_fetch_handler: Handler<String> = hooks.use_async_handler({
        clone_states!(fetch_handler, filter_config);
        move |search_text: String| {
            let fetch_handler = fetch_handler.clone();
            let filter_config = filter_config.clone();
            async move {
                // Debounce wait to prevent excessive API calls while typing
                tokio::time::sleep(tokio::time::Duration::from_millis(
                    REMOTE_SEARCH_DEBOUNCE_MS,
                ))
                .await;

                let filter_config_ref = filter_config.read();
                let current_provider = filter_config_ref.provider;
                let mut query = filter_config_ref.active_filters.clone();

                // Set search text if not empty, otherwise clear it (browse mode)
                if !search_text.is_empty() {
                    query.search_text = Some(search_text);
                } else {
                    query.search_text = None;
                }

                fetch_handler((current_provider, query));
            }
        }
    });

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

    // Clone fetch_handler for use in event handlers
    let fetch_handler_for_events = fetch_handler.clone();

    // Async push handler for pushing local tickets to remote
    let push_handler: Handler<(Vec<String>, Platform, RemoteQuery)> = hooks.use_async_handler({
        clone_states!(local_tickets, fetch_handler, toast, last_error, local_nav);
        move |(ticket_ids, platform, query): (Vec<String>, Platform, RemoteQuery)| {
            let mut local_tickets = local_tickets.clone();
            let fetch_handler = fetch_handler.clone();
            let mut toast = toast.clone();
            let mut last_error = last_error.clone();
            let mut local_nav = local_nav.clone();

            async move {
                let (successes, errors) =
                    super::operations::push_tickets_to_remote(&ticket_ids, platform).await;

                if !errors.is_empty() {
                    let error_msgs: Vec<String> = errors
                        .iter()
                        .map(|e| format!("{}: {}", e.ticket_id(), e.error_message()))
                        .collect();
                    last_error.set(Some(("Push Errors".to_string(), error_msgs.join("\n"))));
                }

                if successes.is_empty() && !errors.is_empty() {
                    toast.set(Some(Toast::error(format!(
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
                    toast.set(Some(Toast::info(msg)));
                } else {
                    // Mixed results - show what succeeded and what failed
                    let success_ids: Vec<&str> =
                        successes.iter().map(|s| s.ticket_id.as_str()).collect();
                    toast.set(Some(Toast::warning(format!(
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
                local_tickets.set(get_all_tickets_from_disk().items);

                // Clear selection via NavigationData method
                let mut new_nav = local_nav.read().clone();
                new_nav.clear_selection();
                local_nav.set(new_nav);

                // Refresh remote issues to show new issues
                fetch_handler((platform, query));
            }
        }
    });

    let push_handler_for_events = push_handler.clone();

    // Handler to apply accepted sync changes
    // NOTE: Defined before sync_fetch_handler so button handlers can reference it
    let sync_apply_handler: Handler<(
        super::sync_preview::SyncPreviewState,
        Platform,
        RemoteQuery,
    )> = hooks.use_async_handler({
        clone_states!(local_tickets, fetch_handler, toast, last_error);
        move |(state, platform, query): (
            super::sync_preview::SyncPreviewState,
            Platform,
            RemoteQuery,
        )| {
            let mut local_tickets = local_tickets.clone();
            let fetch_handler = fetch_handler.clone();
            let mut toast = toast.clone();
            let mut last_error = last_error.clone();

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
                    last_error.set(Some(("SyncApplyError".to_string(), errors.join("\n"))));
                }

                if applied > 0 {
                    toast.set(Some(Toast::info(format!("Applied {applied} change(s)"))));
                    local_tickets.set(get_all_tickets_from_disk().items);
                    fetch_handler((platform, query));
                } else if !errors.is_empty() {
                    toast.set(Some(Toast::error("Failed to apply changes")));
                }
            }
        }
    });

    // Sync preview button action handlers
    // NOTE: Defined before sync_fetch_handler so they can be passed to SyncPreviewState::new

    // Accept current change
    let sync_accept_handler: Handler<()> = hooks.use_async_handler({
        clone_states!(sync_preview, sync_apply_handler, filter_config);
        move |()| {
            let mut sync_preview = sync_preview.clone();
            let sync_apply_handler = sync_apply_handler.clone();
            let filter_config = filter_config.clone();
            async move {
                let preview = sync_preview.read().clone();
                if let Some(mut p) = preview {
                    if !p.accept_current() {
                        // No more changes, apply all accepted
                        let filter_config_ref = filter_config.read();
                        let current_platform = filter_config_ref.provider;
                        let current_query = filter_config_ref.active_filters.clone();
                        sync_apply_handler((p, current_platform, current_query));
                        sync_preview.set(None);
                    } else {
                        sync_preview.set(Some(p));
                    }
                }
            }
        }
    });

    // Skip current change
    let sync_skip_handler: Handler<()> = hooks.use_async_handler({
        clone_states!(sync_preview, sync_apply_handler, filter_config);
        move |()| {
            let mut sync_preview = sync_preview.clone();
            let sync_apply_handler = sync_apply_handler.clone();
            let filter_config = filter_config.clone();
            async move {
                let preview = sync_preview.read().clone();
                if let Some(mut p) = preview {
                    if !p.skip_current() {
                        // No more changes, apply all accepted
                        let filter_config_ref = filter_config.read();
                        let current_platform = filter_config_ref.provider;
                        let current_query = filter_config_ref.active_filters.clone();
                        sync_apply_handler((p, current_platform, current_query));
                        sync_preview.set(None);
                    } else {
                        sync_preview.set(Some(p));
                    }
                }
            }
        }
    });

    // Accept all changes
    let sync_accept_all_handler: Handler<()> = hooks.use_async_handler({
        clone_states!(sync_preview, sync_apply_handler, filter_config);
        move |()| {
            let mut sync_preview = sync_preview.clone();
            let sync_apply_handler = sync_apply_handler.clone();
            let filter_config = filter_config.clone();
            async move {
                let preview = sync_preview.read().clone();
                if let Some(mut p) = preview {
                    p.accept_all();
                    let filter_config_ref = filter_config.read();
                    let current_platform = filter_config_ref.provider;
                    let current_query = filter_config_ref.active_filters.clone();
                    sync_apply_handler((p, current_platform, current_query));
                    sync_preview.set(None);
                }
            }
        }
    });

    // Cancel sync
    let sync_cancel_handler: Handler<()> = hooks.use_async_handler({
        clone_states!(sync_preview, toast);
        move |()| {
            let mut sync_preview = sync_preview.clone();
            let mut toast = toast.clone();
            async move {
                sync_preview.set(None);
                toast.set(Some(Toast::info("Sync cancelled")));
            }
        }
    });

    // Async sync handler for fetching remote data and building changes
    let sync_fetch_handler: Handler<(Vec<String>, Platform)> = hooks.use_async_handler({
        clone_states!(
            sync_preview,
            toast,
            last_error,
            sync_accept_handler,
            sync_skip_handler,
            sync_accept_all_handler,
            sync_cancel_handler
        );
        move |(ticket_ids, platform): (Vec<String>, Platform)| {
            let mut sync_preview = sync_preview.clone();
            let mut toast = toast.clone();
            let mut last_error = last_error.clone();
            let sync_accept_handler = sync_accept_handler.clone();
            let sync_skip_handler = sync_skip_handler.clone();
            let sync_accept_all_handler = sync_accept_all_handler.clone();
            let sync_cancel_handler = sync_cancel_handler.clone();

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
                                        "Failed to build sync changes for {ticket_id}: {e}"
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            error_messages
                                .push(format!("Failed to fetch remote for {ticket_id}: {e}"));
                        }
                    }
                }

                if !error_messages.is_empty() {
                    last_error.set(Some(("SyncError".to_string(), error_messages.join("\n"))));
                }

                if all_changes.is_empty() {
                    toast.set(Some(Toast::info(
                        "No differences found between local and remote",
                    )));
                } else {
                    toast.set(Some(Toast::info(format!(
                        "Found {} change(s) to review",
                        all_changes.len()
                    ))));
                    sync_preview.set(Some(super::sync_preview::SyncPreviewState::new(
                        all_changes,
                        None,
                        None,
                        None,
                        Some(sync_accept_handler),
                        Some(sync_skip_handler),
                        Some(sync_accept_all_handler),
                        Some(sync_cancel_handler),
                    )));
                }
            }
        }
    });

    let sync_fetch_handler_for_events = sync_fetch_handler.clone();
    let sync_apply_handler_for_events = sync_apply_handler.clone();

    // Async link handler for linking a local ticket to a remote issue
    let link_handler: Handler<super::link_mode::LinkSource> = hooks.use_async_handler({
        clone_states!(local_tickets, toast);
        move |source: super::link_mode::LinkSource| {
            let mut local_tickets = local_tickets.clone();
            let mut toast = toast.clone();

            async move {
                match super::operations::link_ticket_to_issue(
                    &source.ticket_id,
                    &source.remote_issue,
                )
                .await
                {
                    Ok(()) => {
                        toast.set(Some(Toast::info(format!(
                            "Linked {} to {}",
                            source.ticket_id, source.remote_issue.id
                        ))));
                        local_tickets.set(get_all_tickets_from_disk().items);
                    }
                    Err(e) => {
                        toast.set(Some(Toast::error(format!("Link failed: {e}"))));
                    }
                }
            }
        }
    });

    let link_handler_for_events = link_handler.clone();

    // Async unlink handler for unlinking local tickets from remote issues
    let unlink_handler: Handler<Vec<String>> = hooks.use_async_handler({
        clone_states!(local_tickets, local_nav, toast);
        move |ticket_ids: Vec<String>| {
            let mut local_tickets = local_tickets.clone();
            let mut local_nav = local_nav.clone();
            let mut toast = toast.clone();

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
                    local_tickets.set(get_all_tickets_from_disk().items);
                    // Clear selection via NavigationData
                    let mut new_nav = local_nav.read().clone();
                    new_nav.clear_selection();
                    local_nav.set(new_nav);
                }

                // Report results
                if errors.is_empty() {
                    toast.set(Some(Toast::info(format!("Unlinked {unlinked} ticket(s)"))));
                } else if unlinked > 0 {
                    // Partial success
                    toast.set(Some(Toast::warning(format!(
                        "Unlinked {}, failed {} (see logs)",
                        unlinked,
                        errors.len()
                    ))));
                    // Log detailed errors
                    for (id, err) in errors {
                        eprintln!("Failed to unlink {id}: {err}");
                    }
                } else {
                    // Total failure
                    if errors.len() == 1 {
                        toast.set(Some(Toast::error(format!(
                            "Failed to unlink: {}",
                            errors[0].1
                        ))));
                    } else {
                        toast.set(Some(Toast::error(format!(
                            "Failed to unlink {} ticket(s) (see logs)",
                            errors.len()
                        ))));
                        // Log detailed errors
                        for (id, err) in errors {
                            eprintln!("Failed to unlink {id}: {err}");
                        }
                    }
                }
            }
        }
    });

    let unlink_handler_for_events = unlink_handler.clone();

    // Click handlers for TabBar - must be defined at top level (not in element! macro)
    let tab_local_click_handler = hooks.use_async_handler({
        clone_states!(view_display);
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
        clone_states!(view_display);
        move |()| {
            let mut view_display = view_display.clone();
            async move {
                let mut new_display = view_display.read().clone();
                new_display.set_view(ViewMode::Remote);
                view_display.set(new_display);
            }
        }
    });

    // Click handler for search box
    let search_click_handler = hooks.use_async_handler({
        clone_states!(search_ui);
        move |()| {
            let mut search_ui = search_ui.clone();
            async move {
                let mut new_ui = search_ui.read().clone();
                new_ui.focused = true;
                search_ui.set(new_ui);
            }
        }
    });

    // Click handler for list pane
    let list_pane_click_handler = hooks.use_async_handler({
        clone_states!(search_ui, view_display);
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

    // Click handlers for list rows
    let local_row_click_handler = hooks.use_async_handler({
        clone_states!(local_nav);
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
        clone_states!(remote_nav);
        move |idx: usize| {
            let mut remote_nav = remote_nav.clone();
            async move {
                let mut new_nav = remote_nav.read().clone();
                new_nav.select_item(idx);
                remote_nav.set(new_nav);
            }
        }
    });

    // Click handler for detail pane
    let detail_pane_click_handler = hooks.use_async_handler({
        clone_states!(view_display);
        move |()| {
            let mut view_display = view_display.clone();
            async move {
                let mut new_display = view_display.read().clone();
                new_display.detail_pane_focused = true;
                view_display.set(new_display);
            }
        }
    });

    // Scroll handlers for detail pane
    let detail_scroll_up_handler = hooks.use_async_handler({
        clone_states!(view_display, detail_scroll);
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
        clone_states!(view_display, detail_scroll);
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

    // Filter modal click handler
    let filter_limit_click_handler = hooks.use_async_handler({
        clone_states!(filter_state);
        move |()| {
            let mut filter_state = filter_state.clone();
            async move {
                let state = filter_state.read().clone();
                if let Some(mut s) = state {
                    s.focused_field = 0;
                    filter_state.set(Some(s));
                }
            }
        }
    });

    // Help modal scroll handlers
    let help_scroll_up_handler = hooks.use_async_handler({
        clone_states!(modal_visibility);
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
        clone_states!(modal_visibility);
        move |()| {
            let mut modal_visibility = modal_visibility.clone();
            async move {
                let mut visibility = modal_visibility.read().clone();
                visibility.help_scroll += 3;
                modal_visibility.set(visibility);
            }
        }
    });

    // Calculate visible list height for scroll/pagination calculations
    // This is NOT for layout (handled by flexbox) but for determining how many
    // items fit in the visible area for keyboard navigation and scroll offset.
    // Additional elements: tabs(1) + search(1) + link_banner(1) + selection_bar(1) + borders(1) = 5
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
    let filtered_local: Vec<super::filter::FilteredLocalTicket> = filtered_tickets
        .iter()
        .map(|ft| super::filter::FilteredLocalTicket {
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
                handlers::handle_key_event(&mut ctx, code, modifiers);
            }
            _ => {}
        }
    });

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

    // Render the UI using sub-components
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
