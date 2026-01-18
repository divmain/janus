//! Issue browser command (`janus view`)
//!
//! Provides an interactive TUI for browsing and managing tickets with
//! fuzzy search and inline editing.

use iocraft::prelude::*;

use crate::error::{JanusError, Result};
use crate::tui::IssueBrowser;

/// Launch the issue browser TUI
///
/// NOTE: This function creates its own tokio runtime because it's an entry point
/// for the TUI. This is intentional and safe since it's not called from within
/// another async context.
pub fn cmd_view() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| JanusError::Other(format!("Failed to create runtime: {}", e)))?;

    rt.block_on(async {
        element!(IssueBrowser)
            .fullscreen()
            .await
            .map_err(|e| JanusError::Other(format!("TUI error: {}", e)))
    })
}
