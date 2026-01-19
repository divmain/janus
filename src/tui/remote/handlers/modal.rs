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
    if ctx.modals.show_error_modal.get() {
        ctx.modals.show_error_modal.set(false);
        return HandleResult::Handled;
    }

    if ctx.modals.show_help_modal.get() {
        ctx.modals.show_help_modal.set(false);
        return HandleResult::Handled;
    }

    if ctx.modals.sync_preview.read().is_some() {
        ctx.modals.sync_preview.set(None);
        return HandleResult::Handled;
    }

    if ctx.filters.filter_modal.read().is_some() {
        ctx.filters.filter_modal.set(None);
        return HandleResult::Handled;
    }

    {
        let link_mode = ctx.modals.link_mode.read().as_ref().cloned();
        if let Some(lm) = link_mode {
            ctx.view_state.active_view.set(lm.source_view);
            ctx.modals.link_mode.set(None);
            return HandleResult::Handled;
        }
    }

    HandleResult::NotHandled
}
