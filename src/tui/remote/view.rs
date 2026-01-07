//! Main remote TUI view component
//!
//! This module provides the main TUI interface for managing local tickets
//! and remote issues with keyboard navigation, list viewing, and detail pane.

// Allow clone on Copy types - used intentionally in async closures for clarity
#![allow(clippy::clone_on_copy)]
#![allow(clippy::redundant_closure)]

use std::collections::HashSet;

use iocraft::prelude::*;

use crate::remote::config::Platform;
use crate::remote::{RemoteIssue, RemoteProvider, RemoteQuery, RemoteStatus};
use crate::ticket::get_all_tickets_from_disk;
use crate::tui::components::{Footer, InlineSearchBox, Shortcut};
use crate::tui::theme::theme;
use crate::types::TicketMetadata;

use super::confirm_modal::ConfirmDialogState;
use super::error_modal::ErrorDetailModal;
use super::error_toast::Toast;
use super::filter::{
    FilteredLocalTicket, FilteredRemoteIssue, filter_local_tickets, filter_remote_issues,
};
use super::filter_modal::{FilterModal, FilterState};
use super::help_modal::HelpModal;
use super::link_mode::LinkModeState;
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
    let mut local_tickets: State<Vec<TicketMetadata>> = hooks.use_state(get_all_tickets_from_disk);
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

    // Operation state
    let mut toast: State<Option<Toast>> = hooks.use_state(|| None);
    let mut link_mode: State<Option<LinkModeState>> = hooks.use_state(|| None);
    let mut confirm_dialog: State<Option<ConfirmDialogState>> = hooks.use_state(|| None);
    let mut sync_preview: State<Option<SyncPreviewState>> = hooks.use_state(|| None);
    let mut show_help_modal = hooks.use_state(|| false);
    let mut show_error_modal = hooks.use_state(|| false);

    // Last error info (for error detail modal) - stores (type, message)
    let last_error: State<Option<(String, String)>> = hooks.use_state(|| None);

    // Search state
    let mut search_query = hooks.use_state(String::new);
    let mut search_focused = hooks.use_state(|| false);

    // Provider state (GitHub or Linear)
    let mut provider = hooks.use_state(|| Platform::GitHub);

    // Filter state
    let mut filter_state: State<Option<FilterState>> = hooks.use_state(|| None);
    let mut active_filters = hooks.use_state(|| RemoteQuery::new());

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
                        .map(|e| format!("{}: {}", e.ticket_id, e.error))
                        .collect();
                    last_error_setter.set(Some(("Push Errors".to_string(), error_msgs.join("\n"))));
                }

                if successes.is_empty() && !errors.is_empty() {
                    toast_setter.set(Some(Toast::error(format!(
                        "Push failed for {}: {}",
                        errors[0].ticket_id, errors[0].error
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
                            .map(|e| e.ticket_id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))));
                }

                // Refresh local tickets to show updated remote links
                local_tickets_setter.set(get_all_tickets_from_disk());

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

                let mut all_changes: Vec<SyncChangeWithContext> = Vec::new();

                for ticket_id in &ticket_ids {
                    match super::operations::fetch_remote_issue_for_ticket(ticket_id, platform)
                        .await
                    {
                        Ok((metadata, issue)) => {
                            let changes = super::operations::build_sync_changes(&metadata, &issue);
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
                            last_error_setter.set(Some((
                                "SyncError".to_string(),
                                format!("Failed to fetch remote for {}: {}", ticket_id, e),
                            )));
                        }
                    }
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
                    local_tickets_setter.set(get_all_tickets_from_disk());
                    fetch_handler((platform, query));
                } else if !errors.is_empty() {
                    toast_setter.set(Some(Toast::error("Failed to apply changes")));
                }
            }
        }
    });

    let sync_apply_handler_for_events = sync_apply_handler.clone();

    // Calculate list height
    let list_height = height.saturating_sub(7) as usize;

    // Get current values for rendering
    let current_view = active_view.get();
    let is_loading = remote_loading.get();
    let detail_visible = show_detail.get();

    // Read collections for rendering
    let local_tickets_ref = local_tickets.read();
    let remote_issues_ref = remote_issues.read();
    let local_selected_ref = local_selected_ids.read();
    let remote_selected_ref = remote_selected_ids.read();

    // Filter based on search query
    let query = search_query.to_string();
    let filtered_local: Vec<FilteredLocalTicket> = filter_local_tickets(&local_tickets_ref, &query);
    let filtered_remote: Vec<FilteredRemoteIssue> =
        filter_remote_issues(&remote_issues_ref, &query);

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

    // Drop refs before events
    drop(local_tickets_ref);
    drop(remote_issues_ref);
    drop(local_selected_ref);
    drop(remote_selected_ref);

    // Keyboard event handling
    hooks.use_terminal_events({
        move |event| match event {
            TerminalEvent::Key(KeyEvent {
                code,
                kind,
                modifiers,
                ..
            }) if kind != KeyEventKind::Release => {
                let shift_held = modifiers.contains(KeyModifiers::SHIFT);
                // Search mode handling
                if search_focused.get() {
                    match code {
                        KeyCode::Esc => {
                            search_query.set(String::new());
                            search_focused.set(false);
                        }
                        KeyCode::Enter | KeyCode::Tab => {
                            search_focused.set(false);
                        }
                        _ => {}
                    }
                    return;
                }

                // Normal mode handling
                match code {
                    KeyCode::Char('q') => {
                        should_exit.set(true);
                    }
                    KeyCode::Char('/') => {
                        search_focused.set(true);
                    }
                    KeyCode::Char('P') => {
                        let new_provider = match provider.get() {
                            Platform::GitHub => Platform::Linear,
                            Platform::Linear => Platform::GitHub,
                        };
                        provider.set(new_provider);
                        // Clear selections when switching providers
                        local_selected_ids.set(HashSet::new());
                        remote_selected_ids.set(HashSet::new());
                        local_selected_index.set(0);
                        remote_selected_index.set(0);
                        local_scroll_offset.set(0);
                        remote_scroll_offset.set(0);
                        remote_issues.set(Vec::new());
                        remote_loading.set(true);
                        // Fetch issues for the new provider
                        let current_query = active_filters.read().clone();
                        fetch_handler_for_events.clone()((new_provider, current_query));
                        toast.set(Some(Toast::info(format!("Switched to {}", new_provider))));
                    }
                    KeyCode::Char('r') => {
                        // Refresh remote issues
                        if !remote_loading.get() {
                            remote_loading.set(true);
                            toast.set(Some(Toast::info("Refreshing remote issues...")));
                            let current_query = active_filters.read().clone();
                            fetch_handler_for_events.clone()((provider.get(), current_query));
                        }
                    }
                    KeyCode::Char('f') => {
                        // Open filter modal
                        if filter_state.read().is_none() {
                            let current_query = active_filters.read().clone();
                            filter_state.set(Some(FilterState::from_query(&current_query)));
                        }
                    }
                    KeyCode::Char('j') | KeyCode::Char('J') | KeyCode::Down => {
                        if active_view.get() == ViewMode::Local {
                            if local_count > 0 {
                                let current_idx = local_selected_index.get();
                                let new_idx = (current_idx + 1).min(local_count - 1);

                                // If shift is held, extend selection to include current item
                                if shift_held {
                                    let tickets = local_tickets.read();
                                    if let Some(ticket) = tickets.get(current_idx)
                                        && let Some(id) = &ticket.id
                                    {
                                        let id = id.clone();
                                        drop(tickets);
                                        let mut ids = local_selected_ids.read().clone();
                                        ids.insert(id);
                                        local_selected_ids.set(ids);
                                    }
                                }

                                local_selected_index.set(new_idx);
                                if new_idx >= local_scroll_offset.get() + list_height {
                                    local_scroll_offset
                                        .set(new_idx.saturating_sub(list_height - 1));
                                }

                                // Also select new item if shift is held
                                if shift_held {
                                    let tickets = local_tickets.read();
                                    if let Some(ticket) = tickets.get(new_idx)
                                        && let Some(id) = &ticket.id
                                    {
                                        let id = id.clone();
                                        drop(tickets);
                                        let mut ids = local_selected_ids.read().clone();
                                        ids.insert(id);
                                        local_selected_ids.set(ids);
                                    }
                                }
                            } else {
                                local_selected_index.set(0);
                            }
                        } else if remote_count > 0 {
                            let current_idx = remote_selected_index.get();
                            let new_idx = (current_idx + 1).min(remote_count - 1);

                            // If shift is held, extend selection to include current item
                            if shift_held {
                                let issues = remote_issues.read();
                                if let Some(issue) = issues.get(current_idx) {
                                    let id = issue.id.clone();
                                    drop(issues);
                                    let mut ids = remote_selected_ids.read().clone();
                                    ids.insert(id);
                                    remote_selected_ids.set(ids);
                                }
                            }

                            remote_selected_index.set(new_idx);
                            if new_idx >= remote_scroll_offset.get() + list_height {
                                remote_scroll_offset.set(new_idx.saturating_sub(list_height - 1));
                            }

                            // Also select new item if shift is held
                            if shift_held {
                                let issues = remote_issues.read();
                                if let Some(issue) = issues.get(new_idx) {
                                    let id = issue.id.clone();
                                    drop(issues);
                                    let mut ids = remote_selected_ids.read().clone();
                                    ids.insert(id);
                                    remote_selected_ids.set(ids);
                                }
                            }
                        } else {
                            remote_selected_index.set(0);
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Char('K') | KeyCode::Up => {
                        if active_view.get() == ViewMode::Local {
                            let current_idx = local_selected_index.get();
                            let new_idx = current_idx.saturating_sub(1);

                            // If shift is held, extend selection to include current item
                            if shift_held {
                                let tickets = local_tickets.read();
                                if let Some(ticket) = tickets.get(current_idx)
                                    && let Some(id) = &ticket.id
                                {
                                    let id = id.clone();
                                    drop(tickets);
                                    let mut ids = local_selected_ids.read().clone();
                                    ids.insert(id);
                                    local_selected_ids.set(ids);
                                }
                            }

                            local_selected_index.set(new_idx);
                            if new_idx < local_scroll_offset.get() {
                                local_scroll_offset.set(new_idx);
                            }

                            // Also select new item if shift is held
                            if shift_held {
                                let tickets = local_tickets.read();
                                if let Some(ticket) = tickets.get(new_idx)
                                    && let Some(id) = &ticket.id
                                {
                                    let id = id.clone();
                                    drop(tickets);
                                    let mut ids = local_selected_ids.read().clone();
                                    ids.insert(id);
                                    local_selected_ids.set(ids);
                                }
                            }
                        } else {
                            let current_idx = remote_selected_index.get();
                            let new_idx = current_idx.saturating_sub(1);

                            // If shift is held, extend selection to include current item
                            if shift_held {
                                let issues = remote_issues.read();
                                if let Some(issue) = issues.get(current_idx) {
                                    let id = issue.id.clone();
                                    drop(issues);
                                    let mut ids = remote_selected_ids.read().clone();
                                    ids.insert(id);
                                    remote_selected_ids.set(ids);
                                }
                            }

                            remote_selected_index.set(new_idx);
                            if new_idx < remote_scroll_offset.get() {
                                remote_scroll_offset.set(new_idx);
                            }

                            // Also select new item if shift is held
                            if shift_held {
                                let issues = remote_issues.read();
                                if let Some(issue) = issues.get(new_idx) {
                                    let id = issue.id.clone();
                                    drop(issues);
                                    let mut ids = remote_selected_ids.read().clone();
                                    ids.insert(id);
                                    remote_selected_ids.set(ids);
                                }
                            }
                        }
                    }
                    KeyCode::Char('g') => {
                        if active_view.get() == ViewMode::Local {
                            local_selected_index.set(0);
                            local_scroll_offset.set(0);
                        } else {
                            remote_selected_index.set(0);
                            remote_scroll_offset.set(0);
                        }
                    }
                    KeyCode::Char('G') => {
                        if active_view.get() == ViewMode::Local {
                            if local_count > 0 {
                                let new_idx = local_count - 1;
                                local_selected_index.set(new_idx);
                                if new_idx >= list_height {
                                    local_scroll_offset
                                        .set(new_idx.saturating_sub(list_height - 1));
                                }
                            }
                        } else if remote_count > 0 {
                            let new_idx = remote_count - 1;
                            remote_selected_index.set(new_idx);
                            if new_idx >= list_height {
                                remote_scroll_offset.set(new_idx.saturating_sub(list_height - 1));
                            }
                        }
                    }
                    KeyCode::Char(' ') => {
                        if active_view.get() == ViewMode::Local {
                            let tickets = local_tickets.read();
                            if let Some(ticket) = tickets.get(local_selected_index.get())
                                && let Some(id) = &ticket.id
                            {
                                let id = id.clone();
                                drop(tickets);
                                let mut ids = local_selected_ids.read().clone();
                                if ids.contains(&id) {
                                    ids.remove(&id);
                                } else {
                                    ids.insert(id);
                                }
                                local_selected_ids.set(ids);
                            }
                        } else {
                            let issues = remote_issues.read();
                            if let Some(issue) = issues.get(remote_selected_index.get()) {
                                let id = issue.id.clone();
                                drop(issues);
                                let mut ids = remote_selected_ids.read().clone();
                                if ids.contains(&id) {
                                    ids.remove(&id);
                                } else {
                                    ids.insert(id);
                                }
                                remote_selected_ids.set(ids);
                            }
                        }
                    }
                    KeyCode::Char('a') => {
                        // If sync preview is open, accept all changes
                        if sync_preview.read().is_some() {
                            let mut preview = sync_preview.read().clone().unwrap();
                            preview.accept_all();
                            // Apply all accepted changes
                            let current_platform = provider.get();
                            let current_query = active_filters.read().clone();
                            sync_apply_handler_for_events.clone()((
                                preview,
                                current_platform,
                                current_query,
                            ));
                            sync_preview.set(None);
                        } else if active_view.get() == ViewMode::Remote {
                            // Adopt remote issues
                            let selected_ids: Vec<String> =
                                remote_selected_ids.read().iter().cloned().collect();
                            if !selected_ids.is_empty() {
                                let issues: Vec<RemoteIssue> = remote_issues
                                    .read()
                                    .iter()
                                    .filter(|i| selected_ids.contains(&i.id))
                                    .cloned()
                                    .collect();

                                match super::operations::adopt_issues(
                                    &issues,
                                    &local_selected_ids.read(),
                                ) {
                                    Ok(ids) => {
                                        toast.set(Some(Toast::info(format!(
                                            "Adopted {} issues",
                                            ids.len()
                                        ))));
                                        local_tickets.set(get_all_tickets_from_disk());
                                        remote_selected_ids.set(HashSet::new());
                                    }
                                    Err(e) => {
                                        toast.set(Some(Toast::error(format!(
                                            "Adopt failed: {}",
                                            e
                                        ))));
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Char('c') => {
                        // Cancel sync preview (same as Esc when sync preview is open)
                        if sync_preview.read().is_some() {
                            sync_preview.set(None);
                            toast.set(Some(Toast::info("Sync cancelled")));
                        }
                    }
                    KeyCode::Char('l') => {
                        if link_mode.read().is_none() {
                            // Start link mode - select source item
                            if active_view.get() == ViewMode::Local {
                                let tickets = local_tickets.read();
                                if let Some(ticket) = tickets.get(local_selected_index.get())
                                    && let Some(id) = &ticket.id
                                {
                                    let title = ticket.title.as_deref().unwrap_or("").to_string();
                                    let id_clone = id.clone();
                                    drop(tickets);
                                    link_mode.set(Some(LinkModeState::new(
                                        ViewMode::Local,
                                        id_clone,
                                        title,
                                    )));
                                    active_view.set(ViewMode::Remote);
                                }
                            } else {
                                let issues = remote_issues.read();
                                if let Some(issue) = issues.get(remote_selected_index.get()) {
                                    let lm = LinkModeState::new(
                                        ViewMode::Remote,
                                        issue.id.clone(),
                                        issue.title.clone(),
                                    );
                                    drop(issues);
                                    link_mode.set(Some(lm));
                                    active_view.set(ViewMode::Local);
                                }
                            }
                        } else {
                            // Complete link operation
                            let lm = link_mode.read().clone().unwrap();
                            if lm.source_view == ViewMode::Local {
                                // Source is local ticket, target is remote issue
                                let issues = remote_issues.read();
                                if let Some(issue) =
                                    issues.get(remote_selected_index.get()).cloned()
                                {
                                    drop(issues);
                                    match super::operations::link_ticket_to_issue(
                                        &lm.source_id,
                                        &issue,
                                    ) {
                                        Ok(()) => {
                                            toast.set(Some(Toast::info(format!(
                                                "Linked {} to {}",
                                                lm.source_id, issue.id
                                            ))));
                                            local_tickets.set(get_all_tickets_from_disk());
                                        }
                                        Err(e) => {
                                            toast.set(Some(Toast::error(format!(
                                                "Link failed: {}",
                                                e
                                            ))));
                                        }
                                    }
                                }
                            } else {
                                // Source is remote issue, target is local ticket
                                let tickets = local_tickets.read();
                                if let Some(ticket) =
                                    tickets.get(local_selected_index.get()).cloned()
                                {
                                    drop(tickets);
                                    if let Some(ticket_id) = &ticket.id {
                                        // Find the source remote issue
                                        let issues = remote_issues.read();
                                        if let Some(source_issue) =
                                            issues.iter().find(|i| i.id == lm.source_id).cloned()
                                        {
                                            drop(issues);
                                            match super::operations::link_ticket_to_issue(
                                                ticket_id,
                                                &source_issue,
                                            ) {
                                                Ok(()) => {
                                                    toast.set(Some(Toast::info(format!(
                                                        "Linked {} to {}",
                                                        ticket_id, source_issue.id
                                                    ))));
                                                    local_tickets.set(get_all_tickets_from_disk());
                                                }
                                                Err(e) => {
                                                    toast.set(Some(Toast::error(format!(
                                                        "Link failed: {}",
                                                        e
                                                    ))));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            link_mode.set(None);
                            active_view.set(lm.source_view);
                        }
                    }
                    KeyCode::Char('u') => {
                        // Unlink selected local tickets
                        if active_view.get() == ViewMode::Local {
                            let selected_ids: Vec<String> =
                                local_selected_ids.read().iter().cloned().collect();
                            if !selected_ids.is_empty() {
                                let mut unlinked = 0;
                                for id in &selected_ids {
                                    if super::operations::unlink_ticket(id).is_ok() {
                                        unlinked += 1;
                                    }
                                }
                                if unlinked > 0 {
                                    toast.set(Some(Toast::info(format!(
                                        "Unlinked {} ticket(s)",
                                        unlinked
                                    ))));
                                    local_tickets.set(get_all_tickets_from_disk());
                                    local_selected_ids.set(HashSet::new());
                                }
                            } else {
                                // Unlink current item
                                let tickets = local_tickets.read();
                                if let Some(ticket) = tickets.get(local_selected_index.get())
                                    && let Some(id) = &ticket.id
                                {
                                    let id = id.clone();
                                    drop(tickets);
                                    match super::operations::unlink_ticket(&id) {
                                        Ok(()) => {
                                            toast
                                                .set(Some(Toast::info(format!("Unlinked {}", id))));
                                            local_tickets.set(get_all_tickets_from_disk());
                                        }
                                        Err(e) => {
                                            toast.set(Some(Toast::error(format!(
                                                "Unlink failed: {}",
                                                e
                                            ))));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Char('p') => {
                        // Push local tickets to remote (create remote issues)
                        if active_view.get() == ViewMode::Local {
                            let selected_ids: Vec<String> =
                                local_selected_ids.read().iter().cloned().collect();
                            let tickets_to_push = if !selected_ids.is_empty() {
                                selected_ids
                            } else {
                                // Push current item if no selection
                                let tickets = local_tickets.read();
                                if let Some(ticket) = tickets.get(local_selected_index.get())
                                    && let Some(id) = &ticket.id
                                {
                                    vec![id.clone()]
                                } else {
                                    vec![]
                                }
                            };

                            if tickets_to_push.is_empty() {
                                toast.set(Some(Toast::warning("No ticket selected to push")));
                            } else {
                                // Check if any tickets are already linked
                                let tickets_ref = local_tickets.read();
                                let already_linked: Vec<String> = tickets_to_push
                                    .iter()
                                    .filter(|id| {
                                        tickets_ref.iter().any(|t| {
                                            t.id.as_ref() == Some(*id) && t.remote.is_some()
                                        })
                                    })
                                    .cloned()
                                    .collect();
                                drop(tickets_ref);

                                if !already_linked.is_empty() {
                                    toast.set(Some(Toast::warning(format!(
                                        "{} ticket(s) already linked, skipping",
                                        already_linked.len()
                                    ))));
                                    // Filter out already linked tickets
                                    let unlinked: Vec<String> = tickets_to_push
                                        .into_iter()
                                        .filter(|id| !already_linked.contains(id))
                                        .collect();
                                    if !unlinked.is_empty() {
                                        toast.set(Some(Toast::info(format!(
                                            "Pushing {} ticket(s)...",
                                            unlinked.len()
                                        ))));
                                        let current_query = active_filters.read().clone();
                                        push_handler_for_events.clone()((
                                            unlinked,
                                            provider.get(),
                                            current_query,
                                        ));
                                    }
                                } else {
                                    toast.set(Some(Toast::info(format!(
                                        "Pushing {} ticket(s)...",
                                        tickets_to_push.len()
                                    ))));
                                    let current_query = active_filters.read().clone();
                                    push_handler_for_events.clone()((
                                        tickets_to_push,
                                        provider.get(),
                                        current_query,
                                    ));
                                }
                            }
                        }
                    }
                    KeyCode::Char('s') => {
                        // Sync selected items (only linked tickets)
                        if sync_preview.read().is_none() {
                            let selected_ids: Vec<String> = if active_view.get() == ViewMode::Local
                            {
                                local_selected_ids.read().iter().cloned().collect()
                            } else {
                                // Get local tickets linked to selected remote issues
                                let selected_remote: HashSet<String> =
                                    remote_selected_ids.read().iter().cloned().collect();
                                local_tickets
                                    .read()
                                    .iter()
                                    .filter(|t| {
                                        t.remote.as_ref().is_some_and(|r| {
                                            selected_remote.iter().any(|sr| r.contains(sr))
                                        })
                                    })
                                    .filter_map(|t| t.id.clone())
                                    .collect()
                            };

                            let tickets_to_sync = if !selected_ids.is_empty() {
                                // Filter to only linked tickets
                                let tickets = local_tickets.read();
                                selected_ids
                                    .into_iter()
                                    .filter(|id| {
                                        tickets.iter().any(|t| {
                                            t.id.as_ref() == Some(id) && t.remote.is_some()
                                        })
                                    })
                                    .collect::<Vec<_>>()
                            } else {
                                // Sync current item if linked
                                let tickets = local_tickets.read();
                                if let Some(ticket) = tickets.get(local_selected_index.get())
                                    && ticket.remote.is_some()
                                    && let Some(id) = &ticket.id
                                {
                                    vec![id.clone()]
                                } else {
                                    vec![]
                                }
                            };

                            if tickets_to_sync.is_empty() {
                                toast.set(Some(Toast::warning(
                                    "No linked tickets selected for sync",
                                )));
                            } else {
                                toast.set(Some(Toast::info(format!(
                                    "Fetching remote data for {} ticket(s)...",
                                    tickets_to_sync.len()
                                ))));
                                sync_fetch_handler_for_events.clone()((
                                    tickets_to_sync,
                                    provider.get(),
                                ));
                            }
                        }
                    }
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        // Accept current sync change
                        if sync_preview.read().is_some() {
                            let mut preview = sync_preview.read().clone().unwrap();
                            if !preview.accept_current() {
                                // No more changes, apply all accepted
                                let current_platform = provider.get();
                                let current_query = active_filters.read().clone();
                                sync_apply_handler_for_events.clone()((
                                    preview,
                                    current_platform,
                                    current_query,
                                ));
                                sync_preview.set(None);
                            } else {
                                sync_preview.set(Some(preview));
                            }
                        }
                    }
                    KeyCode::Char('n') => {
                        // Skip current sync change (but not when filter modal is open)
                        if sync_preview.read().is_some() && filter_state.read().is_none() {
                            let mut preview = sync_preview.read().clone().unwrap();
                            if !preview.skip_current() {
                                // No more changes, apply all accepted
                                let current_platform = provider.get();
                                let current_query = active_filters.read().clone();
                                sync_apply_handler_for_events.clone()((
                                    preview,
                                    current_platform,
                                    current_query,
                                ));
                                sync_preview.set(None);
                            } else {
                                sync_preview.set(Some(preview));
                            }
                        }
                    }
                    KeyCode::Esc => {
                        if show_error_modal.get() {
                            show_error_modal.set(false);
                        } else if show_help_modal.get() {
                            show_help_modal.set(false);
                        } else if confirm_dialog.read().is_some() {
                            confirm_dialog.set(None);
                        } else if sync_preview.read().is_some() {
                            sync_preview.set(None);
                        } else if filter_state.read().is_some() {
                            filter_state.set(None);
                        } else if link_mode.read().is_some() {
                            active_view.set(link_mode.read().as_ref().unwrap().source_view);
                            link_mode.set(None);
                        }
                    }
                    KeyCode::Tab => {
                        // Tab navigation in filter modal, or switch views
                        if filter_state.read().is_some() {
                            let mut state = filter_state.read().clone().unwrap();
                            state.focus_next();
                            filter_state.set(Some(state));
                        } else {
                            let new_view = match active_view.get() {
                                ViewMode::Local => ViewMode::Remote,
                                ViewMode::Remote => ViewMode::Local,
                            };
                            active_view.set(new_view);
                        }
                    }
                    KeyCode::BackTab => {
                        // Shift+Tab in filter modal
                        if filter_state.read().is_some() {
                            let mut state = filter_state.read().clone().unwrap();
                            state.focus_prev();
                            filter_state.set(Some(state));
                        }
                    }
                    KeyCode::Char('x') => {
                        // Clear filters in filter modal
                        if filter_state.read().is_some() {
                            let mut state = filter_state.read().clone().unwrap();
                            state.clear();
                            filter_state.set(Some(state));
                        }
                    }
                    KeyCode::Enter => {
                        // Apply filters in filter modal, or toggle status field, or toggle detail
                        if filter_state.read().is_some() {
                            let state = filter_state.read().clone().unwrap();
                            if state.focused_field == 0 {
                                // Toggle status
                                let mut new_state = state.clone();
                                new_state.toggle_status();
                                filter_state.set(Some(new_state));
                            } else {
                                // Apply filters
                                let base_query = active_filters.read().clone();
                                let new_query = state.to_query(&base_query);
                                active_filters.set(new_query.clone());
                                filter_state.set(None);
                                // Refresh with new filters
                                remote_loading.set(true);
                                toast.set(Some(Toast::info("Applying filters...")));
                                fetch_handler_for_events.clone()((provider.get(), new_query));
                            }
                        } else {
                            show_detail.set(!show_detail.get());
                        }
                    }
                    KeyCode::Char('?') => {
                        show_help_modal.set(true);
                    }
                    KeyCode::Char('e') => {
                        if last_error.read().is_some() {
                            show_error_modal.set(true);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    });

    // Exit if requested
    if should_exit.get() {
        system.exit();
    }

    // Compute selected items for rendering (after event closure to avoid move issues)
    let filtered_local_data = filter_local_tickets(&local_tickets.read(), &query);
    let filtered_remote_data = filter_remote_issues(&remote_issues.read(), &query);

    let selected_local = filtered_local_data
        .get(local_selected_index.get())
        .map(|f| f.ticket.clone());
    let selected_remote = filtered_remote_data
        .get(remote_selected_index.get())
        .map(|f| f.issue.clone());

    // Shortcuts for footer
    let mut shortcuts = vec![
        Shortcut::new("q", "quit"),
        Shortcut::new("Tab", "switch view"),
        Shortcut::new("j/k", "nav"),
        Shortcut::new("Space", "select"),
        Shortcut::new("/", "search"),
        Shortcut::new("P", "switch provider"),
        Shortcut::new("r", "refresh"),
        Shortcut::new("f", "filter"),
    ];

    if current_view == ViewMode::Remote {
        shortcuts.push(Shortcut::new("a", "adopt"));
    } else {
        shortcuts.push(Shortcut::new("p", "push"));
        shortcuts.push(Shortcut::new("u", "unlink"));
    }

    if link_mode.read().is_some() {
        shortcuts.push(Shortcut::new("l", "confirm link"));
        shortcuts.push(Shortcut::new("Esc", "cancel"));
    } else {
        shortcuts.push(Shortcut::new("l", "link"));
    }

    shortcuts.push(Shortcut::new("s", "sync"));
    shortcuts.push(Shortcut::new("Enter", "toggle detail"));

    // Render the UI
    element! {
        View(
            width,
            height,
            flex_direction: FlexDirection::Column,
            background_color: theme.background,
        ) {
            // Header row
            View(
                width: 100pct,
                padding_left: 1,
                padding_right: 1,
            ) {
                Text(
                    content: "janus remote",
                    color: Color::Cyan,
                    weight: Weight::Bold,
                )
                Text(
                    content: format!(" [{}]", provider.get()),
                    color: theme.text_dimmed,
                )
                View(flex_grow: 1.0)
                Text(content: "[?]", color: theme.text_dimmed)
            }

            // Tab bar
            View(
                width: 100pct,
                padding_left: 1,
                border_edges: Edges::Bottom,
                border_style: BorderStyle::Single,
                border_color: theme.border,
            ) {
                Text(
                    content: "[Local] ",
                    color: if current_view == ViewMode::Local { Color::Cyan } else { theme.text_dimmed },
                    weight: if current_view == ViewMode::Local { Weight::Bold } else { Weight::Normal },
                )
                Text(
                    content: "[Remote] ",
                    color: if current_view == ViewMode::Remote { Color::Cyan } else { theme.text_dimmed },
                    weight: if current_view == ViewMode::Remote { Weight::Bold } else { Weight::Normal },
                )
                View(flex_grow: 1.0)
                #(if query.is_empty() {
                    None
                } else {
                    Some(element! {
                        Text(
                            content: format!(" Filter: {}", query),
                            color: Color::Yellow,
                        )
                    })
                })
            }

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
                )
            }

            // Link mode banner
            #(link_mode.read().as_ref().map(|lm| element! {
                View(
                    width: 100pct,
                    padding_left: 1,
                    padding_right: 1,
                    border_edges: Edges::Bottom,
                    border_style: BorderStyle::Single,
                    border_color: Color::Yellow,
                    background_color: Color::DarkGrey,
                ) {
                    Text(
                        content: format!(
                            "Link {} ({}) -> select target, [l] to confirm, [Esc] to cancel",
                            lm.source_id,
                            lm.source_title
                        ),
                        color: Color::Yellow,
                    )
                }
            }))

            // Main content area
            View(
                flex_grow: 1.0,
                width: 100pct,
                flex_direction: FlexDirection::Row,
            ) {
                // List pane
                View(
                    width: 40pct,
                    height: 100pct,
                    flex_direction: FlexDirection::Column,
                    border_style: BorderStyle::Round,
                    border_color: theme.border_focused,
                ) {
                    #(if current_view == ViewMode::Remote {
                        if is_loading {
                            Some(element! {
                                View(
                                    flex_grow: 1.0,
                                    width: 100pct,
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                ) {
                                    Text(content: "Loading remote issues...", color: theme.text_dimmed)
                                }
                            })
                        } else if remote_count == 0 {
                            Some(element! {
                                View(
                                    flex_grow: 1.0,
                                    width: 100pct,
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                ) {
                                    Text(content: "No remote issues found", color: theme.text_dimmed)
                                }
                            })
                        } else {
                                    Some(element! {
                                        View(
                                            width: 100pct,
                                            height: 100pct,
                                            flex_direction: FlexDirection::Column,
                                        ) {
                                            #(remote_list.iter().enumerate().map(|(i, filtered)| {
                                                let actual_idx = remote_scroll_offset.get() + i;
                                                let is_selected = actual_idx == remote_selected_index.get();
                                                let issue = &filtered.issue;
                                                let is_marked = remote_selected_ids.read().contains(&issue.id);

                                                let status_color = match &issue.status {
                                                    RemoteStatus::Open => Color::Green,
                                                    RemoteStatus::Closed => Color::DarkGrey,
                                                    RemoteStatus::Custom(_) => Color::White,
                                                };

                                                let indicator = if is_selected { ">" } else { " " };
                                                let marker = if is_marked { "*" } else { " " };
                                                let is_linked = local_tickets.read().iter().any(|t| {
                                                    t.remote.as_ref().is_some_and(|r| r.contains(&issue.id))
                                                });
                                                let link_indicator = if is_linked { "⟷" } else { " " };

                                                let status_str = match &issue.status {
                                                    RemoteStatus::Open => "open".to_string(),
                                                    RemoteStatus::Closed => "closed".to_string(),
                                                    RemoteStatus::Custom(s) => s.clone(),
                                                };

                                                let title_display = if issue.title.len() > 25 {
                                                    format!("{}...", &issue.title[..22])
                                                } else {
                                                    issue.title.clone()
                                                };

                                                element! {
                                                    View(
                                                        height: 1,
                                                        width: 100pct,
                                                        padding_left: 1,
                                                        background_color: if is_selected { Some(theme.highlight) } else { None },
                                                    ) {
                                                        Text(content: indicator, color: Color::White)
                                                        Text(content: marker, color: Color::White)
                                                        Text(content: link_indicator, color: Color::Cyan)
                                                        Text(
                                                            content: format!(" {:<10}", &issue.id),
                                                            color: if is_selected { Color::White } else { theme.id_color },
                                                        )
                                                        Text(
                                                            content: format!(" [{}]", status_str),
                                                            color: if is_selected { Color::White } else { status_color },
                                                        )
                                                        Text(
                                                            content: format!(" {}", title_display),
                                                            color: Color::White,
                                                        )
                                                    }
                                                }
                                            }))
                                        }
                                    })
                        }
                    } else {
                        // Local view
                        if local_count == 0 {
                            Some(element! {
                                View(
                                    flex_grow: 1.0,
                                    width: 100pct,
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                ) {
                                    Text(content: "No local tickets", color: theme.text_dimmed)
                                }
                            })
                        } else {
                            Some(element! {
                                View(
                                    width: 100pct,
                                    height: 100pct,
                                    flex_direction: FlexDirection::Column,
                                ) {
                                    #(local_list.iter().enumerate().map(|(i, filtered)| {
                                        let actual_idx = local_scroll_offset.get() + i;
                                        let is_selected = actual_idx == local_selected_index.get();
                                        let ticket = &filtered.ticket;
                                        let ticket_id = ticket.id.as_deref().unwrap_or("???");
                                        let is_marked = local_selected_ids.read().contains(ticket_id);

                                        let status = ticket.status.unwrap_or_default();
                                        let status_color = theme.status_color(status);

                                        let indicator = if is_selected { ">" } else { " " };
                                        let marker = if is_marked { "*" } else { " " };
                                        let link_indicator = if ticket.remote.is_some() { "⟷" } else { " " };

                                        let title = ticket.title.as_deref().unwrap_or("(no title)");
                                        let title_display = if title.len() > 25 {
                                            format!("{}...", &title[..22])
                                        } else {
                                            title.to_string()
                                        };

                                        let status_str = match status {
                                            crate::types::TicketStatus::New => "new",
                                            crate::types::TicketStatus::Next => "nxt",
                                            crate::types::TicketStatus::InProgress => "wip",
                                            crate::types::TicketStatus::Complete => "don",
                                            crate::types::TicketStatus::Cancelled => "can",
                                        };

                                        element! {
                                            View(
                                                height: 1,
                                                width: 100pct,
                                                padding_left: 1,
                                                background_color: if is_selected { Some(theme.highlight) } else { None },
                                            ) {
                                                Text(content: indicator, color: Color::White)
                                                Text(content: marker, color: Color::White)
                                                Text(content: link_indicator, color: Color::Cyan)
                                                Text(
                                                    content: format!(" {:<8}", ticket_id),
                                                    color: if is_selected { Color::White } else { theme.id_color },
                                                )
                                                Text(
                                                    content: format!(" [{}]", status_str),
                                                    color: if is_selected { Color::White } else { status_color },
                                                )
                                                Text(
                                                    content: format!(" {}", title_display),
                                                    color: Color::White,
                                                )
                                            }
                                        }
                                    }))
                                }
                            })
                        }
                    })
                }

                // Detail pane (when visible)
                #(if detail_visible {
                    Some(element! {
                        View(
                            flex_grow: 1.0,
                            height: 100pct,
                            flex_direction: FlexDirection::Column,
                            border_style: BorderStyle::Round,
                            border_color: theme.border,
                        ) {
                            #(if current_view == ViewMode::Remote {
                                if let Some(issue) = &selected_remote {
                                    let status_str = match &issue.status {
                                        RemoteStatus::Open => "open".to_string(),
                                        RemoteStatus::Closed => "closed".to_string(),
                                        RemoteStatus::Custom(s) => s.clone(),
                                    };

                                    Some(element! {
                                        View(
                                            width: 100pct,
                                            height: 100pct,
                                            flex_direction: FlexDirection::Column,
                                            overflow: Overflow::Hidden,
                                        ) {
                                            // Header
                                            View(
                                                width: 100pct,
                                                padding: 1,
                                                border_edges: Edges::Bottom,
                                                border_style: BorderStyle::Single,
                                                border_color: theme.border,
                                            ) {
                                                View(flex_direction: FlexDirection::Column) {
                                                    Text(content: issue.id.clone(), color: theme.id_color, weight: Weight::Bold)
                                                    Text(content: issue.title.clone(), color: theme.text, weight: Weight::Bold)
                                                }
                                            }

                                            // Metadata
                                            View(
                                                width: 100pct,
                                                padding: 1,
                                                flex_direction: FlexDirection::Column,
                                            ) {
                                                Text(content: format!("Status: {}", status_str), color: Color::Green)
                                                Text(content: format!("Priority: {:?}", issue.priority), color: theme.text)
                                                Text(content: format!("Assignee: {:?}", issue.assignee), color: theme.text)
                                                Text(content: format!("Updated: {}", &issue.updated_at[..10.min(issue.updated_at.len())]), color: theme.text)
                                            }

                                            // Body
                                            View(
                                                flex_grow: 1.0,
                                                width: 100pct,
                                                padding: 1,
                                                overflow: Overflow::Hidden,
                                                flex_direction: FlexDirection::Column,
                                            ) {
                                                #(issue.body.lines().take(15).map(|line| {
                                                    element! {
                                                        Text(content: line.to_string(), color: theme.text)
                                                    }
                                                }))
                                            }
                                        }
                                    })
                                } else {
                                    Some(element! {
                                        View(
                                            flex_grow: 1.0,
                                            justify_content: JustifyContent::Center,
                                            align_items: AlignItems::Center,
                                        ) {
                                            Text(content: "No issue selected", color: theme.text_dimmed)
                                        }
                                    })
                                }
                            } else {
                                // Local ticket detail
                                if let Some(ticket) = &selected_local {
                                    let status = ticket.status.unwrap_or_default();

                                    Some(element! {
                                        View(
                                            width: 100pct,
                                            height: 100pct,
                                            flex_direction: FlexDirection::Column,
                                            overflow: Overflow::Hidden,
                                        ) {
                                            View(
                                                width: 100pct,
                                                padding: 1,
                                                border_edges: Edges::Bottom,
                                                border_style: BorderStyle::Single,
                                                border_color: theme.border,
                                            ) {
                                                View(flex_direction: FlexDirection::Column) {
                                                    Text(
                                                        content: ticket.id.clone().unwrap_or_default(),
                                                        color: theme.id_color,
                                                        weight: Weight::Bold,
                                                    )
                                                    Text(
                                                        content: ticket.title.clone().unwrap_or_default(),
                                                        color: theme.text,
                                                        weight: Weight::Bold,
                                                    )
                                                }
                                            }

                                            View(
                                                width: 100pct,
                                                padding: 1,
                                                flex_direction: FlexDirection::Column,
                                            ) {
                                                Text(content: format!("Status: {}", status), color: theme.status_color(status))
                                                Text(content: format!("Type: {:?}", ticket.ticket_type), color: theme.text)
                                                Text(content: format!("Priority: {:?}", ticket.priority), color: theme.text)
                                                Text(content: format!("Assignee: {:?}", ticket.assignee), color: theme.text)
                                            }
                                        }
                                    })
                                } else {
                                    Some(element! {
                                        View(
                                            flex_grow: 1.0,
                                            justify_content: JustifyContent::Center,
                                            align_items: AlignItems::Center,
                                        ) {
                                            Text(content: "No ticket selected", color: theme.text_dimmed)
                                        }
                                    })
                                }
                            })
                        }
                    })
                } else {
                    None
                })
            }

            // Selection status bar
            #(if current_view == ViewMode::Remote && remote_sel_count > 0 {
                Some(element! {
                    View(
                        width: 100pct,
                        padding_left: 1,
                        border_edges: Edges::Top,
                        border_style: BorderStyle::Single,
                        border_color: theme.border,
                    ) {
                        Text(content: format!("{} selected", remote_sel_count), color: Color::Cyan)
                    }
                })
            } else if current_view == ViewMode::Local && local_sel_count > 0 {
                Some(element! {
                    View(
                        width: 100pct,
                        padding_left: 1,
                        border_edges: Edges::Top,
                        border_style: BorderStyle::Single,
                        border_color: theme.border,
                    ) {
                        Text(content: format!("{} selected", local_sel_count), color: Color::Cyan)
                    }
                })
            } else {
                None
            })

            // Footer
            Footer(shortcuts: shortcuts)

            // Toast notification
            #(toast.read().as_ref().map(|t| element! {
                View(
                    width: 100pct,
                    height: 3,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    background_color: Color::Black,
                    border_edges: Edges::Top,
                    border_style: BorderStyle::Single,
                    border_color: t.color(),
                ) {
                    Text(content: t.message.clone(), color: t.color())
                }
            }))

            // Filter modal overlay
            #(filter_state.read().as_ref().map(|state| {
                let state_clone = state.clone();
                element! {
                    View(
                        width: 100pct,
                        height: 100pct,
                        position: Position::Absolute,
                        top: 0,
                        left: 0,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        background_color: Color::DarkGrey,
                    ) {
                        FilterModal(state: state_clone)
                    }
                }
            }))

            // Help modal overlay
            #(if show_help_modal.get() {
                Some(element! {
                    View(
                        width: 100pct,
                        height: 100pct,
                        position: Position::Absolute,
                        top: 0,
                        left: 0,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        background_color: Color::DarkGrey,
                    ) {
                        HelpModal()
                    }
                })
            } else {
                None
            })

            // Error detail modal overlay
            #(if show_error_modal.get() {
                last_error.read().as_ref().map(|(error_type, error_message)| {
                    let error_type_clone = error_type.clone();
                    let error_message_clone = error_message.clone();
                    element! {
                        View(
                            width: 100pct,
                            height: 100pct,
                            position: Position::Absolute,
                            top: 0,
                            left: 0,
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            background_color: Color::DarkGrey,
                        ) {
                            ErrorDetailModal(error_type: error_type_clone.clone(), error_message: error_message_clone.clone())
                        }
                    }
                })
            } else {
                None
            })
        }
    }
}
