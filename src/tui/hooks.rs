//! Reusable hooks for TUI components

use iocraft::prelude::*;

use crate::tui::repository::{InitResult, TicketRepository, janus_dir_exists};
use crate::types::TicketMetadata;

/// Create an async handler for loading tickets with minimum display time
///
/// This hook creates a handler that:
/// - Checks if the Janus directory exists
/// - Loads tickets from the repository
/// - Sets appropriate empty states based on results
/// - Ensures minimum 100ms loading indicator display to prevent UI flicker
///
/// # Returns
///
/// A handler that can be called with `()` to trigger the load operation.
/// The handler updates the provided state setters as tickets are loaded.
///
/// # Example
///
/// ```ignore
/// let init_result: State<InitResult> = hooks.use_state(|| InitResult::Ok);
/// let all_tickets: State<Vec<TicketMetadata>> = hooks.use_state(Vec::new);
/// let mut is_loading = hooks.use_state(|| true);
///
/// let load_handler = hooks.use_async_handler(
///     use_ticket_loader(all_tickets, is_loading, init_result)
/// );
///
/// // Trigger load
/// load_handler(());
/// ```
pub fn use_ticket_loader(
    tickets_setter: State<Vec<TicketMetadata>>,
    loading_setter: State<bool>,
    init_result_setter: State<InitResult>,
) -> impl Fn(()) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Clone {
    move |()| {
        let mut tickets_setter = tickets_setter;
        let mut loading_setter = loading_setter;
        let mut init_result_setter = init_result_setter;

        Box::pin(async move {
            let start = std::time::Instant::now();

            if !janus_dir_exists() {
                init_result_setter.set(InitResult::NoJanusDir);
                loading_setter.set(false);
                return;
            }

            let tickets = TicketRepository::load_tickets().await;

            if tickets.is_empty() {
                init_result_setter.set(InitResult::EmptyDir);
            } else {
                init_result_setter.set(InitResult::Ok);
            }

            // Ensure minimum 100ms display time to prevent flicker
            let elapsed = start.elapsed();
            if elapsed < std::time::Duration::from_millis(100) {
                tokio::time::sleep(std::time::Duration::from_millis(100) - elapsed).await;
            }

            tickets_setter.set(tickets);
            loading_setter.set(false);
        })
    }
}
