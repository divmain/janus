//! Kanban board command (`janus board`)
//!
//! Provides an interactive TUI for viewing tickets organized by status
//! in a kanban-style board layout.

use iocraft::prelude::*;

use crate::archive::sweep_completed_tickets;
use crate::error::{JanusError, Result};
use crate::store::{get_or_init_store, start_watching};
use crate::tui::KanbanBoard;

/// Launch the kanban board TUI
pub async fn cmd_board() -> Result<()> {
    // Initialize store and start filesystem watcher for live updates
    let store = get_or_init_store().await?;
    let _ = start_watching(store).await;

    // Run the auto-archive sweep before launching the TUI so the Complete column
    // doesn't contain tickets that should have rolled off. Failures here are
    // non-fatal — we'd rather show the board than block on a sweep error.
    let tickets = store.get_all_tickets();
    let _ = sweep_completed_tickets(&tickets).await;

    element!(KanbanBoard)
        .fullscreen()
        .await
        .map_err(|e| JanusError::TuiError(format!("{e}")))
}
