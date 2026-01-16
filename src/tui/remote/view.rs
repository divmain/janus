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
use super::handlers::{self, HandlerContext};
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
    let _confirm_dialog: State<Option<ConfirmDialogState>> = hooks.use_state(|| None);
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

    // Keyboard event handling - dispatched to separate handler modules
    hooks.use_terminal_events({
        move |event| match event {
            TerminalEvent::Key(KeyEvent {
                code,
                kind,
                modifiers,
                ..
            }) if kind != KeyEventKind::Release => {
                // Build the handler context with all mutable state references
                let mut ctx = HandlerContext {
                    active_view: &mut active_view,
                    show_detail: &mut show_detail,
                    should_exit: &mut should_exit,
                    local_tickets: &mut local_tickets,
                    remote_issues: &mut remote_issues,
                    local_selected_index: &mut local_selected_index,
                    remote_selected_index: &mut remote_selected_index,
                    local_scroll_offset: &mut local_scroll_offset,
                    remote_scroll_offset: &mut remote_scroll_offset,
                    local_selected_ids: &mut local_selected_ids,
                    remote_selected_ids: &mut remote_selected_ids,
                    local_count,
                    remote_count,
                    list_height,
                    remote_loading: &mut remote_loading,
                    toast: &mut toast,
                    link_mode: &mut link_mode,
                    sync_preview: &mut sync_preview,
                    show_help_modal: &mut show_help_modal,
                    show_error_modal: &mut show_error_modal,
                    last_error: &last_error,
                    search_query: &mut search_query,
                    search_focused: &mut search_focused,
                    provider: &mut provider,
                    filter_state: &mut filter_state,
                    active_filters: &mut active_filters,
                    fetch_handler: &fetch_handler_for_events,
                    push_handler: &push_handler_for_events,
                    sync_fetch_handler: &sync_fetch_handler_for_events,
                    sync_apply_handler: &sync_apply_handler_for_events,
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
