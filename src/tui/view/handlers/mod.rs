//! Keyboard event handlers for the issue browser view
//!
//! This module breaks up the complex event handling logic into separate,
//! focused handlers for each mode or operation type.

mod context;
mod list;
mod navigation;
mod search;
mod types;

pub use context::ViewHandlerContext;
pub use types::ViewAction;

use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::tui::state::Pane;

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
pub fn handle_key_event(ctx: &mut ViewHandlerContext<'_>, code: KeyCode, modifiers: KeyModifiers) {
    // 1. Search mode has highest priority - captures all input
    if ctx.active_pane.get() == Pane::Search && search::handle(ctx, code, modifiers).is_handled() {
        return;
    }

    // 2. Navigation (j/k/g/G/Up/Down/PageUp/PageDown) - works in List and Detail
    if matches!(ctx.active_pane.get(), Pane::List | Pane::Detail)
        && navigation::handle(ctx, code).is_handled()
    {
        return;
    }

    // 3. Mode-specific operations
    match ctx.active_pane.get() {
        Pane::List => {
            list::handle_list(ctx, code);
        }
        Pane::Detail => {
            list::handle_detail(ctx, code);
        }
        Pane::Search => {
            // Already handled above
        }
    }
}
