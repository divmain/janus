//! Sync operation handler factories for the remote TUI
//!
//! This module provides factory functions for sync-related async handlers.
//! These handlers work together to manage the sync preview flow.

use futures::stream::{self, StreamExt};
use iocraft::hooks::UseAsyncHandler;
use iocraft::prelude::{Handler, Hooks, State};

use crate::remote::{Platform, RemoteQuery};

use super::super::error_toast::Toast;
use super::super::operations::{build_sync_changes, fetch_remote_issue_for_ticket};
use super::super::sync_preview::{SyncChangeWithContext, SyncPreviewState};

/// Factory for creating the sync fetch handler
///
/// This is the main handler that fetches sync preview data. It requires the
/// sync action handlers to be passed in as parameters.
#[allow(clippy::too_many_arguments)]
pub fn create_sync_fetch_handler(
    hooks: &mut Hooks,
    sync_preview: &State<Option<SyncPreviewState>>,
    toast: &State<Option<Toast>>,
    last_error: &State<Option<(String, String)>>,
    sync_accept_handler: &Handler<()>,
    sync_skip_handler: &Handler<()>,
    sync_accept_all_handler: &Handler<()>,
    sync_cancel_handler: &Handler<()>,
    filter_config: &State<super::super::state::FilterConfigData>,
) -> Handler<(Vec<String>, Platform)> {
    let sync_preview = *sync_preview;
    let toast = *toast;
    let last_error = *last_error;
    let sync_accept_handler = sync_accept_handler.clone();
    let sync_skip_handler = sync_skip_handler.clone();
    let sync_accept_all_handler = sync_accept_all_handler.clone();
    let sync_cancel_handler = sync_cancel_handler.clone();
    let filter_config = *filter_config;

    hooks.use_async_handler(move |(ticket_ids, platform): (Vec<String>, Platform)| {
        let mut sync_preview = sync_preview;
        let mut toast = toast;
        let mut last_error = last_error;
        let sync_accept_handler = sync_accept_handler.clone();
        let sync_skip_handler = sync_skip_handler.clone();
        let sync_accept_all_handler = sync_accept_all_handler.clone();
        let sync_cancel_handler = sync_cancel_handler.clone();
        let _filter_config = filter_config;

        async move {
            let owned_ticket_ids: Vec<String> = ticket_ids.to_vec();
            let mut all_changes: Vec<SyncChangeWithContext> = Vec::new();
            let mut error_messages: Vec<String> = Vec::new();

            let results: Vec<_> = stream::iter(owned_ticket_ids)
                .map(|ticket_id| async {
                    let result = fetch_remote_issue_for_ticket(&ticket_id, platform).await;
                    (ticket_id, result)
                })
                .buffer_unordered(5)
                .collect()
                .await;

            for (ticket_id, fetch_result) in results {
                match fetch_result {
                    Ok((metadata, issue)) => match build_sync_changes(&metadata, &issue) {
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
                            error_messages
                                .push(format!("Failed to build sync changes for {ticket_id}: {e}"));
                        }
                    },
                    Err(e) => {
                        error_messages.push(format!("Failed to fetch remote for {ticket_id}: {e}"));
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
                sync_preview.set(Some(SyncPreviewState::new(
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
    })
}

/// Factory for creating sync action handlers
///
/// These handlers manage the sync preview UI state and trigger apply when complete.
pub fn create_sync_action_handlers(
    hooks: &mut Hooks,
    sync_preview: &State<Option<SyncPreviewState>>,
    sync_apply_handler: &Handler<(SyncPreviewState, Platform, RemoteQuery)>,
    filter_config: &State<super::super::state::FilterConfigData>,
    toast: &State<Option<Toast>>,
) -> SyncActionHandlers {
    let sync_preview_accept = *sync_preview;
    let sync_preview_skip = *sync_preview;
    let sync_preview_accept_all = *sync_preview;
    let sync_preview_cancel = *sync_preview;
    let sync_apply_accept = sync_apply_handler.clone();
    let sync_apply_skip = sync_apply_handler.clone();
    let sync_apply_accept_all = sync_apply_handler.clone();
    let filter_config_accept = *filter_config;
    let filter_config_skip = *filter_config;
    let filter_config_accept_all = *filter_config;
    let toast_cancel = *toast;

    let accept = hooks.use_async_handler(move |()| {
        let mut sync_preview = sync_preview_accept;
        let sync_apply_handler = sync_apply_accept.clone();
        let filter_config = filter_config_accept;
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
    });

    let skip = hooks.use_async_handler(move |()| {
        let mut sync_preview = sync_preview_skip;
        let sync_apply_handler = sync_apply_skip.clone();
        let filter_config = filter_config_skip;
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
    });

    let accept_all = hooks.use_async_handler(move |()| {
        let mut sync_preview = sync_preview_accept_all;
        let sync_apply_handler = sync_apply_accept_all.clone();
        let filter_config = filter_config_accept_all;
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
    });

    let cancel = hooks.use_async_handler(move |()| {
        let mut sync_preview = sync_preview_cancel;
        let mut toast = toast_cancel;
        async move {
            sync_preview.set(None);
            toast.set(Some(Toast::info("Sync cancelled")));
        }
    });

    SyncActionHandlers {
        accept,
        skip,
        accept_all,
        cancel,
    }
}

/// Sync action handlers bundle
pub struct SyncActionHandlers {
    pub accept: Handler<()>,
    pub skip: Handler<()>,
    pub accept_all: Handler<()>,
    pub cancel: Handler<()>,
}
