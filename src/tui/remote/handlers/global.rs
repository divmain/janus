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
        KeyCode::Char('q') | KeyCode::Esc => {
            ctx.view_state.should_exit.set(true);
            HandleResult::Handled
        }
        KeyCode::Char('/') => {
            ctx.search.focused.set(true);
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
            if ctx.modals.sync_preview.read().is_none() {
                sync::handle_start_sync(ctx);
            }
            HandleResult::Handled
        }
        KeyCode::Char('?') => {
            ctx.modals.show_help_modal.set(true);
            HandleResult::Handled
        }
        KeyCode::Char('e') => {
            if ctx.modals.last_error.read().is_some() {
                ctx.modals.show_error_modal.set(true);
            }
            HandleResult::Handled
        }
        KeyCode::Enter => {
            // Toggle detail pane focus or visibility (only when no modal is open)
            if ctx.filters.filter_modal.read().is_none() {
                let detail_focused = ctx.view_state.detail_pane_focused.get();
                if detail_focused {
                    // When detail is focused, press Enter to focus back on list
                    ctx.view_state.detail_pane_focused.set(false);
                } else {
                    // Toggle detail pane visibility
                    let detail_visible = ctx.view_state.show_detail.get();
                    if detail_visible {
                        // Detail visible but not focused - focus it
                        ctx.view_state.detail_pane_focused.set(true);
                    } else {
                        // Show detail pane and focus it
                        ctx.view_state.show_detail.set(true);
                        ctx.view_state.detail_pane_focused.set(true);
                    }
                }
            }
            HandleResult::Handled
        }
        KeyCode::Tab => {
            // Switch views (only when filter modal is not open)
            if ctx.filters.filter_modal.read().is_none() {
                let new_view = match ctx.view_state.active_view.get() {
                    ViewMode::Local => ViewMode::Remote,
                    ViewMode::Remote => ViewMode::Local,
                };
                ctx.view_state.active_view.set(new_view);
            }
            HandleResult::Handled
        }
        _ => HandleResult::NotHandled,
    }
}

fn handle_switch_provider(ctx: &mut HandlerContext<'_>) {
    use crate::remote::config::Platform;

    let new_provider = match ctx.filters.provider.get() {
        Platform::GitHub => Platform::Linear,
        Platform::Linear => Platform::GitHub,
    };
    ctx.filters.provider.set(new_provider);

    // Clear selections when switching providers
    ctx.view_data.local_nav.selected_ids.set(HashSet::new());
    ctx.view_data.remote_nav.selected_ids.set(HashSet::new());
    ctx.view_data.local_nav.selected_index.set(0);
    ctx.view_data.remote_nav.selected_index.set(0);
    ctx.view_data.local_nav.scroll_offset.set(0);
    ctx.view_data.remote_nav.scroll_offset.set(0);
    ctx.view_data.remote_issues.set(Vec::new());
    ctx.remote.loading.set(true);

    // Fetch issues for the new provider
    let current_query = ctx.filters.active_filters.read().clone();
    ctx.handlers.fetch_handler.clone()((new_provider, current_query));
    ctx.modals
        .toast
        .set(Some(Toast::info(format!("Switched to {new_provider}"))));
}

fn handle_refresh(ctx: &mut HandlerContext<'_>) {
    if !ctx.remote.loading.get() {
        ctx.remote.loading.set(true);
        ctx.modals
            .toast
            .set(Some(Toast::info("Refreshing remote issues...")));
        let current_query = ctx.filters.active_filters.read().clone();
        ctx.handlers.fetch_handler.clone()((ctx.filters.provider.get(), current_query));
    }
}

fn handle_open_filter(ctx: &mut HandlerContext<'_>) {
    // Always derive modal state from active_filters when opening
    // This ensures consistency even if modal was previously cancelled
    let current_query = ctx.filters.active_filters.read().clone();
    ctx.filters
        .filter_modal
        .set(Some(FilterState::from_query(&current_query)));
}
