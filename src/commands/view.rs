//! Issue browser command (`janus view`)
//!
//! Provides an interactive TUI for browsing and managing tickets with
//! fuzzy search and inline editing.

use iocraft::prelude::*;

use crate::error::{JanusError, Result};
use crate::store::{get_or_init_store, start_watching};
use crate::tui::IssueBrowser;

/// Launch the issue browser TUI
pub async fn cmd_view() -> Result<()> {
    // Initialize store and start filesystem watcher for live updates
    let store = get_or_init_store().await?;
    let _ = start_watching(store).await;

    element!(IssueBrowser)
        .fullscreen()
        .await
        .map_err(|e| JanusError::Other(format!("TUI error: {e}")))
}
