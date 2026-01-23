//! Modal dismissal and navigation handler (Esc key, scroll keys)

use iocraft::prelude::KeyCode;

use super::HandleResult;
use super::context::HandlerContext;
use crate::tui::remote::help_modal::help_content_line_count;

/// Handle modal keys (Esc for dismissal, j/k for scroll)
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    // Handle help modal scrolling
    if ctx.modals.show_help_modal.get() {
        match code {
            KeyCode::Char('j') | KeyCode::Down => {
                let current = ctx.modals.help_modal_scroll.get();
                let max_scroll = help_content_line_count().saturating_sub(1);
                if current < max_scroll {
                    ctx.modals.help_modal_scroll.set(current + 1);
                }
                return HandleResult::Handled;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let current = ctx.modals.help_modal_scroll.get();
                if current > 0 {
                    ctx.modals.help_modal_scroll.set(current - 1);
                }
                return HandleResult::Handled;
            }
            KeyCode::Char('g') => {
                ctx.modals.help_modal_scroll.set(0);
                return HandleResult::Handled;
            }
            KeyCode::Char('G') => {
                let max_scroll = help_content_line_count().saturating_sub(1);
                ctx.modals.help_modal_scroll.set(max_scroll);
                return HandleResult::Handled;
            }
            KeyCode::Esc => {
                ctx.modals.show_help_modal.set(false);
                ctx.modals.help_modal_scroll.set(0); // Reset scroll on close
                return HandleResult::Handled;
            }
            _ => return HandleResult::Handled, // Consume other keys when help modal is open
        }
    }

    if code != KeyCode::Esc {
        return HandleResult::NotHandled;
    }

    // Dismiss modals in priority order
    if ctx.modals.show_error_modal.get() {
        ctx.modals.show_error_modal.set(false);
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
