//! Keyboard event handlers for the issue browser view
//!
//! This module breaks up the complex event handling logic into separate,
//! focused handlers for each mode or operation type.

mod context;
mod list;
mod navigation;
mod search;
mod triage;
mod types;

pub use context::{
    AppState, AsyncHandlers, DetailNavigationState, EditState, ListNavigationState, SearchState,
    ViewData, ViewHandlerContext,
};
pub use triage::handle_triage_modal_triggers;

use iocraft::prelude::{KeyCode, KeyModifiers, State};

use crate::error::Result;
use crate::tui::components::Toast;
use crate::tui::repository::TicketRepository;
use crate::tui::state::Pane;
use crate::types::TicketMetadata;

/// Execute a ticket operation with standardized success/error handling and refresh.
///
/// This helper abstracts the common pattern in TUI async handlers:
/// 1. Execute the operation
/// 2. Show success toast on Ok
/// 3. Show error toast on Err
/// 4. Refresh the ticket in the store
/// 5. Refresh the ticket in the local list
///
/// # Type Parameters
/// * `T` - The result type of the operation (used for success message generation)
///
/// # Arguments
/// * `operation` - The async operation to execute
/// * `ticket_id` - The ID of the ticket being modified
/// * `toast_setter` - State setter for displaying toast notifications
/// * `tickets_setter` - State setter for the ticket list
/// * `success_toast` - Function to generate success toast from result
/// * `error_toast` - Function to generate error message from error
pub async fn execute_ticket_op<T>(
    operation: impl std::future::Future<Output = Result<T>>,
    ticket_id: &str,
    toast_setter: &mut State<Option<Toast>>,
    tickets_setter: &mut State<Vec<TicketMetadata>>,
    success_toast: impl FnOnce(&T) -> Toast,
    error_toast: impl FnOnce(&crate::error::JanusError) -> String,
) {
    match operation.await {
        Ok(ref result) => {
            toast_setter.set(Some(success_toast(result)));
            // Refresh the mutated ticket in the store, then update in-place
            TicketRepository::refresh_ticket_in_store(ticket_id).await;
            let current = tickets_setter.read().clone();
            let tickets = TicketRepository::refresh_single_ticket(current, ticket_id).await;
            tickets_setter.set(tickets);
        }
        Err(e) => {
            toast_setter.set(Some(Toast::error(error_toast(&e))));
        }
    }
}

/// Simplified version for operations that return `Result<()>`.
///
/// Uses static success and error messages.
pub async fn execute_ticket_op_simple(
    operation: impl std::future::Future<Output = Result<()>>,
    ticket_id: &str,
    toast_setter: &mut State<Option<Toast>>,
    tickets_setter: &mut State<Vec<TicketMetadata>>,
    success_message: impl AsRef<str>,
    error_prefix: impl AsRef<str>,
) {
    let success_msg = success_message.as_ref().to_string();
    let error_prefix = error_prefix.as_ref().to_string();
    execute_ticket_op(
        operation,
        ticket_id,
        toast_setter,
        tickets_setter,
        |_| Toast::success(&success_msg),
        |e| format!("{error_prefix}: {e}"),
    )
    .await;
}

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
    // 0. Global hotkeys (work in any mode)
    if code == KeyCode::Char('T') && modifiers == KeyModifiers::CONTROL {
        // Toggle triage mode - this is handled in the view component via state
        // So we just return and let the component handle it
        return;
    }

    // 1. Search mode has highest priority - captures all input
    if ctx.app.active_pane.get() == Pane::Search
        && search::handle(ctx, code, modifiers).is_handled()
    {
        return;
    }

    // 2. Navigation (j/k/g/G/Up/Down/PageUp/PageDown) - works in List and Detail
    if matches!(ctx.app.active_pane.get(), Pane::List | Pane::Detail)
        && navigation::handle(ctx, code).is_handled()
    {
        return;
    }

    // 3. Mode-specific operations
    match ctx.app.active_pane.get() {
        Pane::List => {
            list::handle_list(ctx, code, modifiers);
        }
        Pane::Detail => {
            list::handle_detail(ctx, code, modifiers);
        }
        Pane::Search => {
            // Already handled above
        }
    }
}
