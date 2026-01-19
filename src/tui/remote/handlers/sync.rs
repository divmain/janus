//! Sync mode handlers

use std::collections::HashSet;

use iocraft::prelude::KeyCode;

use super::super::error_toast::Toast;
use super::super::state::ViewMode;
use super::HandleResult;
use super::context::HandlerContext;

/// Handle sync preview mode events
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            handle_accept(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('n') => {
            // Only handle 'n' for skip when filter modal is not open
            if ctx.filters.filter_modal.read().is_none() {
                handle_skip(ctx);
                return HandleResult::Handled;
            }
            HandleResult::NotHandled
        }
        KeyCode::Char('a') => {
            handle_accept_all(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('c') => {
            ctx.modals.sync_preview.set(None);
            ctx.modals.toast.set(Some(Toast::info("Sync cancelled")));
            HandleResult::Handled
        }
        _ => HandleResult::NotHandled,
    }
}

fn handle_accept(ctx: &mut HandlerContext<'_>) {
    let preview = ctx.modals.sync_preview.read().clone();
    if let Some(mut p) = preview {
        if !p.accept_current() {
            // No more changes, apply all accepted
            apply_sync_changes(ctx, p);
            ctx.modals.sync_preview.set(None);
        } else {
            ctx.modals.sync_preview.set(Some(p));
        }
    }
}

fn handle_skip(ctx: &mut HandlerContext<'_>) {
    let preview = ctx.modals.sync_preview.read().clone();
    if let Some(mut p) = preview {
        if !p.skip_current() {
            // No more changes, apply all accepted
            apply_sync_changes(ctx, p);
            ctx.modals.sync_preview.set(None);
        } else {
            ctx.modals.sync_preview.set(Some(p));
        }
    }
}

fn handle_accept_all(ctx: &mut HandlerContext<'_>) {
    let preview = ctx.modals.sync_preview.read().clone();
    if let Some(mut p) = preview {
        p.accept_all();
        apply_sync_changes(ctx, p);
        ctx.modals.sync_preview.set(None);
    }
}

fn apply_sync_changes(
    ctx: &mut HandlerContext<'_>,
    preview: super::super::sync_preview::SyncPreviewState,
) {
    let current_platform = ctx.filters.provider.get();
    let current_query = ctx.filters.active_filters.read().clone();
    ctx.handlers.sync_apply_handler.clone()((preview, current_platform, current_query));
}

/// Handle 's' key to start sync operation (called from global handler when sync preview is not open)
pub fn handle_start_sync(ctx: &mut HandlerContext<'_>) {
    let selected_ids: Vec<String> = if ctx.view_state.active_view.get() == ViewMode::Local {
        ctx.view_data
            .local_nav
            .selected_ids
            .read()
            .iter()
            .cloned()
            .collect()
    } else {
        // Get local tickets linked to selected remote issues
        let selected_remote: HashSet<String> = ctx
            .view_data
            .remote_nav
            .selected_ids
            .read()
            .iter()
            .cloned()
            .collect();
        ctx.view_data
            .local_tickets
            .read()
            .iter()
            .filter(|t| {
                t.remote
                    .as_ref()
                    .is_some_and(|r| selected_remote.iter().any(|sr| r.contains(sr)))
            })
            .filter_map(|t| t.id.clone())
            .collect()
    };

    let tickets_to_sync = if !selected_ids.is_empty() {
        // Filter to only linked tickets
        let tickets = ctx.view_data.local_tickets.read();
        selected_ids
            .into_iter()
            .filter(|id| {
                tickets
                    .iter()
                    .any(|t| t.id.as_ref() == Some(id) && t.remote.is_some())
            })
            .collect::<Vec<_>>()
    } else {
        // Sync current item if linked
        let tickets = ctx.view_data.local_tickets.read();
        if let Some(ticket) = tickets.get(ctx.view_data.local_nav.selected_index.get()) {
            if ticket.remote.is_some() {
                if let Some(id) = &ticket.id {
                    vec![id.clone()]
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    };

    if tickets_to_sync.is_empty() {
        ctx.modals
            .toast
            .set(Some(Toast::warning("No linked tickets selected for sync")));
    } else {
        ctx.modals.toast.set(Some(Toast::info(format!(
            "Fetching remote data for {} ticket(s)...",
            tickets_to_sync.len()
        ))));
        ctx.handlers.sync_fetch_handler.clone()((tickets_to_sync, ctx.filters.provider.get()));
    }
}
