use serde_json::json;

use super::CommandOutput;
use crate::cli::OutputOptions;
use crate::error::Result;
use crate::events::{clear_events, events_file_path, read_events};

/// Prune/clear the events log file.
///
/// This command removes all events from the events.ndjson file, effectively
/// resetting the event log. This is useful when the log has grown too large
/// or when you want to start fresh with event tracking.
pub async fn cmd_events_prune(output: OutputOptions) -> Result<()> {
    // Get the events file path for display
    let path = events_file_path();

    // Count events before clearing
    let events = read_events().map_err(|e| {
        crate::error::JanusError::Io(std::io::Error::new(
            e.kind(),
            format!("Failed to read events file: {e}"),
        ))
    })?;
    let event_count = events.len();

    // Clear the events
    clear_events().map_err(|e| {
        crate::error::JanusError::Io(std::io::Error::new(
            e.kind(),
            format!("Failed to clear events file: {e}"),
        ))
    })?;

    let text = if event_count == 0 {
        format!(
            "Events log cleared (no events were present in {})",
            crate::utils::format_relative_path(&path)
        )
    } else {
        format!(
            "Events log cleared successfully. Removed {} event(s) from {}.",
            event_count,
            crate::utils::format_relative_path(&path)
        )
    };

    CommandOutput::new(json!({
        "action": "events_prune",
        "removed_count": event_count,
        "success": true,
    }))
    .with_text(text)
    .print(output)?;

    Ok(())
}
