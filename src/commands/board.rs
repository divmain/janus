//! Kanban board command (`janus board`)
//!
//! Provides an interactive TUI for viewing tickets organized by status
//! in a kanban-style board layout.

use iocraft::prelude::*;

use crate::error::{JanusError, Result};
use crate::tui::KanbanBoard;

/// Launch the kanban board TUI
pub fn cmd_board() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| JanusError::Other(format!("Failed to create runtime: {}", e)))?;

    rt.block_on(async {
        element!(KanbanBoard)
            .fullscreen()
            .await
            .map_err(|e| JanusError::Other(format!("TUI error: {}", e)))
    })
}
