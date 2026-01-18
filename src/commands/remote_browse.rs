use crate::error::{JanusError, Result};

/// TUI for managing remote issues
///
/// NOTE: This function creates its own tokio runtime because it's an entry point
/// for the TUI. This is intentional and safe since it's not called from within
/// another async context.
pub fn cmd_remote_browse(provider: Option<&str>) -> Result<()> {
    use crate::tui::remote::RemoteTui;
    use iocraft::prelude::*;

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| JanusError::Other(format!("Failed to create runtime: {}", e)))?;

    rt.block_on(async {
        element!(RemoteTui(
            provider: provider.map(|p| p.to_string()),
        ))
        .fullscreen()
        .await
        .map_err(|e| JanusError::Other(format!("TUI error: {}", e)))
    })
}
