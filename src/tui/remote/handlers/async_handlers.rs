//! Async handler factories for the remote TUI
//!
//! This module provides factory functions for creating async handlers.
//! These handlers contain the business logic for remote operations.
//!
//! Factory functions accept `&mut Hooks` as their first parameter so they can
//! call `hooks.use_async_handler()` internally.

use iocraft::hooks::UseAsyncHandler;
use iocraft::prelude::{Handler, Hooks, State};

use crate::remote::{Platform, RemoteIssue, RemoteProvider, RemoteQuery};
use crate::ticket::get_all_tickets_from_disk;
use crate::types::TicketMetadata;

use super::super::error_toast::Toast;
use super::super::link_mode::LinkSource;
use super::super::operations::{
    apply_sync_change_to_local, apply_sync_change_to_remote, link_ticket_to_issue,
    push_tickets_to_remote, unlink_ticket,
};
use super::super::state::{NavigationData, ViewDisplayData};
use super::super::sync_preview::{SyncDirection, SyncPreviewState};

/// Result from async fetch operation with pagination metadata
#[derive(Clone)]
pub enum FetchResult {
    Success(crate::remote::PaginatedResult<RemoteIssue>),
    Error(String, String), // (error_type, error_message)
}

/// Debounce delay for remote search input in milliseconds
const REMOTE_SEARCH_DEBOUNCE_MS: u64 = 500;

/// Factory for creating the fetch handler
pub fn create_fetch_handler(
    hooks: &mut Hooks,
    remote_issues: &State<Vec<RemoteIssue>>,
    view_display: &State<ViewDisplayData>,
    toast: &State<Option<Toast>>,
    last_error: &State<Option<(String, String)>>,
    last_fetch_result: &State<Option<(FetchResult, bool)>>,
) -> Handler<(Platform, RemoteQuery)> {
    let remote_issues = *remote_issues;
    let view_display = *view_display;
    let toast = *toast;
    let last_error = *last_error;
    let last_fetch_result = *last_fetch_result;

    hooks.use_async_handler(move |(platform, query): (Platform, RemoteQuery)| {
        let mut remote_issues = remote_issues;
        let mut view_display = view_display;
        let mut toast = toast;
        let mut last_error = last_error;
        let mut last_fetch_result = last_fetch_result;

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
                    last_fetch_result.set(Some((FetchResult::Error(err_type, err_msg), is_search)));
                }
            }

            // Update loading state within view_display struct
            let mut new_display = *view_display.read();
            new_display.remote_loading = false;
            view_display.set(new_display);
        }
    })
}

/// Factory for creating the debounced search fetch handler
pub fn create_search_fetch_handler(
    hooks: &mut Hooks,
    fetch_handler: &Handler<(Platform, RemoteQuery)>,
    filter_config: &State<super::super::state::FilterConfigData>,
) -> Handler<String> {
    let fetch_handler = fetch_handler.clone();
    let filter_config = *filter_config;

    hooks.use_async_handler(move |search_text: String| {
        let fetch_handler = fetch_handler.clone();
        let filter_config = filter_config;

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
    })
}

/// Factory for creating the push handler
pub fn create_push_handler(
    hooks: &mut Hooks,
    local_tickets: &State<Vec<TicketMetadata>>,
    fetch_handler: &Handler<(Platform, RemoteQuery)>,
    toast: &State<Option<Toast>>,
    last_error: &State<Option<(String, String)>>,
    local_nav: &State<NavigationData>,
) -> Handler<(Vec<String>, Platform, RemoteQuery)> {
    let local_tickets = *local_tickets;
    let fetch_handler = fetch_handler.clone();
    let toast = *toast;
    let last_error = *last_error;
    let local_nav = *local_nav;

    hooks.use_async_handler(
        move |(ticket_ids, platform, query): (Vec<String>, Platform, RemoteQuery)| {
            let mut local_tickets = local_tickets;
            let fetch_handler = fetch_handler.clone();
            let mut toast = toast;
            let mut last_error = last_error;
            let mut local_nav = local_nav;

            async move {
                let (successes, errors) = push_tickets_to_remote(&ticket_ids, platform).await;

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
        },
    )
}

/// Factory for creating the link handler
pub fn create_link_handler(
    hooks: &mut Hooks,
    local_tickets: &State<Vec<TicketMetadata>>,
    toast: &State<Option<Toast>>,
) -> Handler<LinkSource> {
    let local_tickets = *local_tickets;
    let toast = *toast;

    hooks.use_async_handler(move |source: LinkSource| {
        let mut local_tickets = local_tickets;
        let mut toast = toast;

        async move {
            match link_ticket_to_issue(&source.ticket_id, &source.remote_issue).await {
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
    })
}

/// Factory for creating the unlink handler
pub fn create_unlink_handler(
    hooks: &mut Hooks,
    local_tickets: &State<Vec<TicketMetadata>>,
    local_nav: &State<NavigationData>,
    toast: &State<Option<Toast>>,
) -> Handler<Vec<String>> {
    let local_tickets = *local_tickets;
    let local_nav = *local_nav;
    let toast = *toast;

    hooks.use_async_handler(move |ticket_ids: Vec<String>| {
        let mut local_tickets = local_tickets;
        let mut local_nav = local_nav;
        let mut toast = toast;

        async move {
            let mut unlinked = 0;
            let mut errors: Vec<(String, String)> = Vec::new();

            for id in &ticket_ids {
                match unlink_ticket(id).await {
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
    })
}

/// Fetch remote issues from the given provider with optional query filters
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

/// Factory for creating the sync apply handler
pub fn create_sync_apply_handler(
    hooks: &mut Hooks,
    local_tickets: &State<Vec<TicketMetadata>>,
    fetch_handler: &Handler<(Platform, RemoteQuery)>,
    toast: &State<Option<Toast>>,
    last_error: &State<Option<(String, String)>>,
) -> Handler<(SyncPreviewState, Platform, RemoteQuery)> {
    let local_tickets = *local_tickets;
    let fetch_handler = fetch_handler.clone();
    let toast = *toast;
    let last_error = *last_error;

    hooks.use_async_handler(
        move |(state, platform, query): (SyncPreviewState, Platform, RemoteQuery)| {
            let mut local_tickets = local_tickets;
            let fetch_handler = fetch_handler.clone();
            let mut toast = toast;
            let mut last_error = last_error;

            async move {
                let accepted = state.accepted_changes();
                let mut applied = 0;
                let mut errors = Vec::new();

                for change_ctx in accepted {
                    let result = match change_ctx.change.direction {
                        SyncDirection::RemoteToLocal => {
                            apply_sync_change_to_local(&change_ctx.ticket_id, &change_ctx.change)
                                .await
                        }
                        SyncDirection::LocalToRemote => {
                            apply_sync_change_to_remote(
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
        },
    )
}
