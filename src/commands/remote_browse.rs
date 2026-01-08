use crate::error::{JanusError, Result};

/// TUI for managing remote issues
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
