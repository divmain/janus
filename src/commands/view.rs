//! Issue browser command (`janus view`)
//!
//! Provides an interactive TUI for browsing and managing tickets with
//! fuzzy search and inline editing.

use iocraft::prelude::*;

use crate::error::{JanusError, Result};
use crate::tui::IssueBrowser;

/// Launch the issue browser TUI
pub async fn cmd_view() -> Result<()> {
    element!(IssueBrowser)
        .fullscreen()
        .await
        .map_err(|e| JanusError::Other(format!("TUI error: {}", e)))
}
