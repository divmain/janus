//! Keyboard event handlers for the remote TUI
//!
//! This module breaks up the complex event handling logic into separate,
//! focused handlers for each mode or operation type.

mod confirm_mode;
pub mod context;
mod filter_mode;
mod global;
mod link;
mod modal;
mod navigation;
mod selection;
mod sync;

pub use context::HandlerContext;

use iocraft::prelude::{KeyCode, KeyModifiers};

use super::state::ViewMode;

/// Result from handling an event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HandleResult {
    /// Event was handled, stop processing
    Handled,
    /// Event was not handled, continue to next handler
    #[default]
    NotHandled,
}

impl HandleResult {
    pub fn is_handled(self) -> bool {
        matches!(self, HandleResult::Handled)
    }
}

/// Main event dispatcher that routes events to the appropriate handler
pub fn handle_key_event(ctx: &mut HandlerContext<'_>, code: KeyCode, modifiers: KeyModifiers) {
    let shift_held = modifiers.contains(KeyModifiers::SHIFT);

    // 1. Search mode has highest priority - captures all input
    if ctx.search.focused.get() && search_mode::handle(ctx, code).is_handled() {
        return;
    }

    // 2. Modal dismissal (Esc) - handles all modal states
    if modal::handle(ctx, code).is_handled() {
        return;
    }

    // 3. Sync preview mode - when sync preview is open
    if ctx.modals.sync_preview.read().is_some() && sync::handle(ctx, code).is_handled() {
        return;
    }

    // 4. Filter modal mode - when filter modal is open
    if ctx.filters.filter_modal.read().is_some() && filter_mode::handle(ctx, code).is_handled() {
        return;
    }

    // 5. Confirm dialog mode - when confirm dialog is open
    if ctx.modals.confirm_dialog.read().is_some() && confirm_mode::handle(ctx, code).is_handled() {
        return;
    }

    // 6. Link mode - when link mode is active
    if ctx.modals.link_mode.read().is_some() && link::handle_link_mode(ctx, code).is_handled() {
        return;
    }

    // 7. Navigation (j/k/g/G/Up/Down)
    if navigation::handle(ctx, code, shift_held).is_handled() {
        return;
    }

    // 8. Selection (Space)
    if selection::handle(ctx, code).is_handled() {
        return;
    }

    // 9. View-specific operations
    if ctx.view_state.active_view.get() == ViewMode::Local {
        if local_ops::handle(ctx, code).is_handled() {
            return;
        }
    } else if remote_ops::handle(ctx, code).is_handled() {
        return;
    }

    // 10. Link initiation
    if link::handle_link_start(ctx, code).is_handled() {
        return;
    }

    // 11. Global operations (q, /, P, r, f, s, ?, e, Enter, Tab)
    global::handle(ctx, code);
}

mod local_ops;
mod remote_ops;
mod search_mode;
