//! Confirm dialog mode handler

use iocraft::prelude::KeyCode;

use super::HandleResult;
use super::context::HandlerContext;
use crate::tui::remote::confirm_modal::ConfirmAction;
use crate::tui::remote::error_toast::Toast;

/// Handle key events when confirm dialog is open
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            // Execute the confirmed action
            if let Some(state) = ctx.modals.confirm_dialog.read().clone() {
                match state.action {
                    ConfirmAction::Unlink(ticket_ids) => {
                        ctx.modals.toast.set(Some(Toast::info(format!(
                            "Unlinking {} ticket(s)...",
                            ticket_ids.len()
                        ))));
                        ctx.handlers.unlink_handler.clone()(ticket_ids);
                    }
                }
            }
            // Close the dialog
            ctx.modals.confirm_dialog.set(None);
            HandleResult::Handled
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('c') | KeyCode::Char('C') => {
            // Cancel - just close the dialog
            ctx.modals.confirm_dialog.set(None);
            HandleResult::Handled
        }
        KeyCode::Esc => {
            // Escape also cancels
            ctx.modals.confirm_dialog.set(None);
            HandleResult::Handled
        }
        _ => {
            // Consume all other keys when confirm dialog is open
            HandleResult::Handled
        }
    }
}
