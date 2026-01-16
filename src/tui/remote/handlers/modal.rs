//! Modal dismissal handler (Esc key)

use iocraft::prelude::KeyCode;

use super::HandleResult;
use super::context::HandlerContext;

/// Handle Esc key for modal dismissal
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    if code != KeyCode::Esc {
        return HandleResult::NotHandled;
    }

    // Dismiss modals in priority order
    if ctx.show_error_modal.get() {
        ctx.show_error_modal.set(false);
        return HandleResult::Handled;
    }

    if ctx.show_help_modal.get() {
        ctx.show_help_modal.set(false);
        return HandleResult::Handled;
    }

    if ctx.sync_preview.read().is_some() {
        ctx.sync_preview.set(None);
        return HandleResult::Handled;
    }

    if ctx.filter_state.read().is_some() {
        ctx.filter_state.set(None);
        return HandleResult::Handled;
    }

    if ctx.link_mode.read().is_some() {
        let source_view = ctx.link_mode.read().as_ref().unwrap().source_view;
        ctx.active_view.set(source_view);
        ctx.link_mode.set(None);
        return HandleResult::Handled;
    }

    HandleResult::NotHandled
}
