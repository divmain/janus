//! Kanban board command (`janus board`)
//!
//! Provides an interactive TUI for viewing tickets organized by status
//! in a kanban-style board layout.

use iocraft::prelude::*;

use crate::error::{JanusError, Result};
use crate::store::{get_or_init_store, start_watching};
use crate::tui::KanbanBoard;

/// Launch the kanban board TUI
pub async fn cmd_board() -> Result<()> {
    // Initialize store and start filesystem watcher for live updates
    let store = get_or_init_store().await?;
    let _ = start_watching(store).await;

    element!(KanbanBoard)
        .fullscreen()
        .await
        .map_err(|e| JanusError::Other(format!("TUI error: {e}")))
}
