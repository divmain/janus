//! Keyboard event handlers for the kanban board view
//!
//! This module breaks up the complex event handling logic into separate,
//! focused handlers for each mode or operation type.

mod actions;
mod column;
mod context;
mod navigation;
mod search;

pub use context::BoardHandlerContext;

use iocraft::prelude::{KeyCode, KeyModifiers};

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
pub fn handle_key_event(ctx: &mut BoardHandlerContext<'_>, code: KeyCode, modifiers: KeyModifiers) {
    // 1. Search mode has highest priority - captures all input
    if ctx.search_focused.get() && search::handle(ctx, code, modifiers).is_handled() {
        return;
    }

    // 2. Navigation (h/l/j/k/arrows)
    if navigation::handle(ctx, code).is_handled() {
        return;
    }

    // 3. Column toggles (1-5)
    if column::handle_toggle(ctx, code).is_handled() {
        return;
    }

    // 4. Status movement (s/S)
    if column::handle_status_move(ctx, code).is_handled() {
        return;
    }

    // 5. Actions (e, n, q, /)
    actions::handle(ctx, code);
}
