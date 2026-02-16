//! Keyboard event handlers for the remote TUI
//!
//! This module breaks up the complex event handling logic into separate,
//! focused handlers for each mode or operation type.
//!
//! Key events are first mapped to a `RemoteAction` by the `keymap` module,
//! then dispatched to the appropriate handler function.

pub mod async_handlers;
mod confirm_mode;
pub mod context;
mod filter_mode;
mod global;
pub mod keymap;
mod link;
mod local_ops;
mod modal;
mod navigation;
mod remote_ops;
mod search_mode;
mod selection;
mod sync;
pub mod sync_handlers;

pub use context::HandlerContext;

use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::tui::handlers::HandleResult;
use crate::tui::navigation as shared_nav;

use super::help_modal::help_content_line_count;
use super::state::ViewMode;

/// Main event dispatcher that routes key events to the appropriate handler.
///
/// 1. Build a read-only snapshot of modal state.
/// 2. Map `(KeyCode, KeyModifiers)` → `RemoteAction` via the keymap.
/// 3. Dispatch the action to the relevant handler function.
pub fn handle_key_event(ctx: &mut HandlerContext<'_>, code: KeyCode, modifiers: KeyModifiers) {
    let modal_state = ctx.modal_state_snapshot();

    if let Some(action) = keymap::key_to_action(code, modifiers, &modal_state) {
        dispatch_action(ctx, action);
    }
}

/// Dispatch a `RemoteAction` to the correct handler.
fn dispatch_action(ctx: &mut HandlerContext<'_>, action: keymap::RemoteAction) {
    use keymap::RemoteAction;

    match action {
        // ==================================================================
        // Navigation
        // ==================================================================
        RemoteAction::MoveUp => {
            navigation::handle(ctx, KeyCode::Up, false);
        }
        RemoteAction::MoveDown => {
            navigation::handle(ctx, KeyCode::Down, false);
        }
        RemoteAction::MoveUpExtendSelection => {
            navigation::handle(ctx, KeyCode::Up, true);
        }
        RemoteAction::MoveDownExtendSelection => {
            navigation::handle(ctx, KeyCode::Down, true);
        }
        RemoteAction::GoToTop => {
            navigation::handle(ctx, KeyCode::Char('g'), false);
        }
        RemoteAction::GoToBottom => {
            navigation::handle(ctx, KeyCode::Char('G'), false);
        }
        RemoteAction::PageUp => {
            handle_page_up(ctx);
        }
        RemoteAction::PageDown => {
            handle_page_down(ctx);
        }

        // ==================================================================
        // View
        // ==================================================================
        RemoteAction::ToggleView => {
            global::handle(ctx, KeyCode::Tab);
        }
        RemoteAction::ToggleDetail => {
            ctx.view_state.toggle_show_detail();
        }

        // ==================================================================
        // Selection
        // ==================================================================
        RemoteAction::ToggleSelection => {
            selection::handle(ctx, KeyCode::Char(' '));
        }
        RemoteAction::SelectAll => {
            handle_select_all(ctx);
        }
        RemoteAction::ClearSelection => {
            handle_clear_selection(ctx);
        }

        // ==================================================================
        // Search
        // ==================================================================
        RemoteAction::FocusSearch => {
            ctx.search.set_focused(true);
        }
        RemoteAction::ExitSearch => {
            ctx.search.orchestrator.trigger_pending();
            ctx.search.set_focused(false);
        }
        RemoteAction::ClearSearchAndExit => {
            ctx.search.set_query(String::new());
            ctx.search.set_focused(false);
        }

        // ==================================================================
        // Help modal
        // ==================================================================
        RemoteAction::ShowHelp => {
            ctx.modals.set_show_help(true);
        }
        RemoteAction::HideHelp => {
            ctx.modals.set_show_help(false);
            ctx.modals.set_help_scroll(0);
        }
        RemoteAction::ScrollHelpUp => {
            let current = ctx.modals.help_scroll();
            if current > 0 {
                ctx.modals.set_help_scroll(current - 1);
            }
        }
        RemoteAction::ScrollHelpDown => {
            let current = ctx.modals.help_scroll();
            let max_scroll = help_content_line_count().saturating_sub(1);
            if current < max_scroll {
                ctx.modals.set_help_scroll(current + 1);
            }
        }
        RemoteAction::ScrollHelpToTop => {
            ctx.modals.set_help_scroll(0);
        }
        RemoteAction::ScrollHelpToBottom => {
            let max_scroll = help_content_line_count().saturating_sub(1);
            ctx.modals.set_help_scroll(max_scroll);
        }

        // ==================================================================
        // Error modal
        // ==================================================================
        RemoteAction::ToggleErrorModal => {
            ctx.modals.toggle_error();
        }

        // ==================================================================
        // Filter modal
        // ==================================================================
        RemoteAction::ShowFilterModal => {
            global::handle(ctx, KeyCode::Char('f'));
        }
        RemoteAction::HideFilterModal => {
            ctx.filters.filter_modal.set(None);
        }
        RemoteAction::FilterTab => {
            filter_mode::handle(ctx, KeyCode::Tab);
        }
        RemoteAction::FilterBackTab => {
            filter_mode::handle(ctx, KeyCode::BackTab);
        }
        RemoteAction::FilterClear => {
            filter_mode::handle(ctx, KeyCode::Char('x'));
        }
        RemoteAction::FilterEnter => {
            filter_mode::handle(ctx, KeyCode::Enter);
        }
        RemoteAction::FilterMoveDown => {
            filter_mode::handle(ctx, KeyCode::Down);
        }
        RemoteAction::FilterMoveUp => {
            filter_mode::handle(ctx, KeyCode::Up);
        }

        // ==================================================================
        // Dismiss modal (generic Esc for detail pane unfocus, etc.)
        // ==================================================================
        RemoteAction::DismissModal => {
            // When detail pane is focused, Esc unfocuses it
            if ctx.view_state.detail_pane_focused() {
                ctx.view_state.set_detail_pane_focused(false);
            }
        }

        // ==================================================================
        // Detail pane scrolling
        // ==================================================================
        RemoteAction::DetailScrollUp => {
            navigation::handle(ctx, KeyCode::Up, false);
        }
        RemoteAction::DetailScrollDown => {
            navigation::handle(ctx, KeyCode::Down, false);
        }
        RemoteAction::DetailScrollToTop => {
            navigation::handle(ctx, KeyCode::Char('g'), false);
        }
        RemoteAction::DetailScrollToBottom => {
            navigation::handle(ctx, KeyCode::Char('G'), false);
        }

        // ==================================================================
        // Confirm dialog
        // ==================================================================
        RemoteAction::ConfirmYes => {
            confirm_mode::handle(ctx, KeyCode::Char('y'));
        }
        RemoteAction::ConfirmNo => {
            confirm_mode::handle(ctx, KeyCode::Esc);
        }

        // ==================================================================
        // Sync preview
        // ==================================================================
        RemoteAction::StartSync => {
            sync::handle_start_sync(ctx);
        }
        RemoteAction::SyncAccept => {
            sync::handle(ctx, KeyCode::Char('y'));
        }
        RemoteAction::SyncSkip => {
            sync::handle(ctx, KeyCode::Char('n'));
        }
        RemoteAction::SyncAcceptAll => {
            sync::handle(ctx, KeyCode::Char('a'));
        }
        RemoteAction::SyncCancel => {
            ctx.modals.sync_preview.set(None);
        }

        // ==================================================================
        // Link mode
        // ==================================================================
        RemoteAction::StartLinkMode => {
            link::handle_link_start(ctx, KeyCode::Char('l'));
        }
        RemoteAction::CancelLinkMode => {
            let link_mode = ctx.modals.link_mode.read().as_ref().cloned();
            if let Some(lm) = link_mode {
                ctx.view_state.set_active_view(lm.source_view);
            }
            ctx.modals.link_mode.set(None);
        }
        RemoteAction::LinkConfirm => {
            link::handle_link_mode(ctx, KeyCode::Char('l'));
        }

        // ==================================================================
        // Operations
        // ==================================================================
        RemoteAction::Refresh => {
            global::handle(ctx, KeyCode::Char('r'));
        }
        RemoteAction::SwitchProvider => {
            global::handle(ctx, KeyCode::Char('P'));
        }
        RemoteAction::Adopt => {
            remote_ops::handle(ctx, KeyCode::Char('a'));
        }
        RemoteAction::PushLocal => {
            local_ops::handle(ctx, KeyCode::Char('p'));
        }
        RemoteAction::UnlinkLocal => {
            local_ops::handle(ctx, KeyCode::Char('u'));
        }

        // ==================================================================
        // App
        // ==================================================================
        RemoteAction::Quit => {
            ctx.view_state.set_should_exit(true);
        }

        // ==================================================================
        // Consumed (key was matched but requires no further action)
        // ==================================================================
        RemoteAction::Consumed => {}
    }
}

// ============================================================================
// Helpers for actions without existing handler functions
// ============================================================================

/// Handle PageUp using the shared navigation utilities.
fn handle_page_up(ctx: &mut HandlerContext<'_>) {
    if ctx.view_state.active_view() == ViewMode::Local {
        let mut selected = ctx.view_data.local_nav.selected_index();
        let mut scroll = ctx.view_data.local_nav.scroll_offset();
        shared_nav::page_up(&mut selected, &mut scroll, ctx.view_data.list_height);
        ctx.view_data.local_nav.set_selected_index(selected);
        ctx.view_data.local_nav.set_scroll_offset(scroll);
    } else {
        let mut selected = ctx.view_data.remote_nav.selected_index();
        let mut scroll = ctx.view_data.remote_nav.scroll_offset();
        shared_nav::page_up(&mut selected, &mut scroll, ctx.view_data.list_height);
        ctx.view_data.remote_nav.set_selected_index(selected);
        ctx.view_data.remote_nav.set_scroll_offset(scroll);
    }
}

/// Handle PageDown using the shared navigation utilities.
fn handle_page_down(ctx: &mut HandlerContext<'_>) {
    if ctx.view_state.active_view() == ViewMode::Local {
        let mut selected = ctx.view_data.local_nav.selected_index();
        let mut scroll = ctx.view_data.local_nav.scroll_offset();
        shared_nav::page_down(
            &mut selected,
            &mut scroll,
            ctx.view_data.local_count,
            ctx.view_data.list_height,
        );
        ctx.view_data.local_nav.set_selected_index(selected);
        ctx.view_data.local_nav.set_scroll_offset(scroll);
    } else {
        let mut selected = ctx.view_data.remote_nav.selected_index();
        let mut scroll = ctx.view_data.remote_nav.scroll_offset();
        shared_nav::page_down(
            &mut selected,
            &mut scroll,
            ctx.view_data.remote_count,
            ctx.view_data.list_height,
        );
        ctx.view_data.remote_nav.set_selected_index(selected);
        ctx.view_data.remote_nav.set_scroll_offset(scroll);
    }
}

/// Select all items in the current view.
fn handle_select_all(ctx: &mut HandlerContext<'_>) {
    if ctx.view_state.active_view() == ViewMode::Local {
        let tickets = ctx.view_data.local_tickets.read();
        let mut ids = ctx.view_data.local_nav.selected_ids();
        for ticket in tickets.iter() {
            if let Some(id) = &ticket.id {
                ids.insert(id.to_string());
            }
        }
        drop(tickets);
        ctx.view_data.local_nav.set_selected_ids(ids);
    } else {
        let issues = ctx.view_data.remote_issues.read();
        let mut ids = ctx.view_data.remote_nav.selected_ids();
        for issue in issues.iter() {
            ids.insert(issue.id.clone());
        }
        drop(issues);
        ctx.view_data.remote_nav.set_selected_ids(ids);
    }
}

/// Clear all selections in the current view.
fn handle_clear_selection(ctx: &mut HandlerContext<'_>) {
    if ctx.view_state.active_view() == ViewMode::Local {
        ctx.view_data.local_nav.clear_selection();
    } else {
        ctx.view_data.remote_nav.clear_selection();
    }
}
