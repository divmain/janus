use iocraft::prelude::*;

use crate::error::{JanusError, Result};
use crate::tui::remote::RemoteTui;

/// TUI for managing remote issues
pub async fn cmd_remote_browse(provider: Option<&str>) -> Result<()> {
    element!(RemoteTui(
        provider: provider.map(|p| p.to_string()),
    ))
    .fullscreen()
    .await
    .map_err(|e| JanusError::Other(format!("TUI error: {}", e)))
}
