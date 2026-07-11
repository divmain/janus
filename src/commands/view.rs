//! Issue browser command (`janus view`)
//!
//! Provides an interactive TUI for browsing and managing tickets with
//! fuzzy search and inline editing.

use iocraft::prelude::*;

use crate::error::{JanusError, Result};
use crate::store::{get_or_init_store, start_watching, stop_watching};
use crate::tui::IssueBrowser;

/// Launch the issue browser TUI
pub async fn cmd_view() -> Result<()> {
    // Initialize store and start filesystem watcher for live updates
    let store = get_or_init_store().await?;
    let _ = start_watching(store).await;

    let result = element!(IssueBrowser)
        .fullscreen()
        .await
        .map_err(|e| JanusError::TuiError(format!("{e}")));

    // Stop the watcher to release OS-level file watch handles (FSEvents
    // streams on macOS, inotify descriptors on Linux). Without this,
    // resources accumulate across process invocations.
    stop_watching();

    result
}
