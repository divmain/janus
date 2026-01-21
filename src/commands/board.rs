//! Kanban board command (`janus board`)
//!
//! Provides an interactive TUI for viewing tickets organized by status
//! in a kanban-style board layout.

use iocraft::prelude::*;

use crate::error::{JanusError, Result};
use crate::tui::KanbanBoard;

/// Launch the kanban board TUI
pub async fn cmd_board() -> Result<()> {
    element!(KanbanBoard)
        .fullscreen()
        .await
        .map_err(|e| JanusError::Other(format!("TUI error: {}", e)))
}
