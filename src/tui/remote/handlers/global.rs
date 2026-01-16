//! Global keyboard handlers (q, /, P, r, f, ?, e, Enter, Tab, etc.)

use std::collections::HashSet;

use iocraft::prelude::KeyCode;

use super::super::error_toast::Toast;
use super::super::filter_modal::FilterState;
use super::super::state::ViewMode;
use super::HandleResult;
use super::context::HandlerContext;
use super::sync;

/// Handle global keys that work in most contexts
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Char('q') => {
            ctx.should_exit.set(true);
            HandleResult::Handled
        }
        KeyCode::Char('/') => {
            ctx.search_focused.set(true);
            HandleResult::Handled
        }
        KeyCode::Char('P') => {
            handle_switch_provider(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('r') => {
            handle_refresh(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('f') => {
            handle_open_filter(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('s') => {
            // Start sync (only when sync preview is not open)
            if ctx.sync_preview.read().is_none() {
                sync::handle_start_sync(ctx);
            }
            HandleResult::Handled
        }
        KeyCode::Char('?') => {
            ctx.show_help_modal.set(true);
            HandleResult::Handled
        }
        KeyCode::Char('e') => {
            if ctx.last_error.read().is_some() {
                ctx.show_error_modal.set(true);
            }
            HandleResult::Handled
        }
        KeyCode::Enter => {
            // Toggle detail pane (only when no modal is open)
            if ctx.filter_state.read().is_none() {
                ctx.show_detail.set(!ctx.show_detail.get());
            }
            HandleResult::Handled
        }
        KeyCode::Tab => {
            // Switch views (only when filter modal is not open)
            if ctx.filter_state.read().is_none() {
                let new_view = match ctx.active_view.get() {
                    ViewMode::Local => ViewMode::Remote,
                    ViewMode::Remote => ViewMode::Local,
                };
                ctx.active_view.set(new_view);
            }
            HandleResult::Handled
        }
        _ => HandleResult::NotHandled,
    }
}

fn handle_switch_provider(ctx: &mut HandlerContext<'_>) {
    use crate::remote::config::Platform;

    let new_provider = match ctx.provider.get() {
        Platform::GitHub => Platform::Linear,
        Platform::Linear => Platform::GitHub,
    };
    ctx.provider.set(new_provider);

    // Clear selections when switching providers
    ctx.local_selected_ids.set(HashSet::new());
    ctx.remote_selected_ids.set(HashSet::new());
    ctx.local_selected_index.set(0);
    ctx.remote_selected_index.set(0);
    ctx.local_scroll_offset.set(0);
    ctx.remote_scroll_offset.set(0);
    ctx.remote_issues.set(Vec::new());
    ctx.remote_loading.set(true);

    // Fetch issues for the new provider
    let current_query = ctx.active_filters.read().clone();
    ctx.fetch_handler.clone()((new_provider, current_query));
    ctx.toast
        .set(Some(Toast::info(format!("Switched to {}", new_provider))));
}

fn handle_refresh(ctx: &mut HandlerContext<'_>) {
    if !ctx.remote_loading.get() {
        ctx.remote_loading.set(true);
        ctx.toast
            .set(Some(Toast::info("Refreshing remote issues...")));
        let current_query = ctx.active_filters.read().clone();
        ctx.fetch_handler.clone()((ctx.provider.get(), current_query));
    }
}

fn handle_open_filter(ctx: &mut HandlerContext<'_>) {
    if ctx.filter_state.read().is_none() {
        let current_query = ctx.active_filters.read().clone();
        ctx.filter_state
            .set(Some(FilterState::from_query(&current_query)));
    }
}
