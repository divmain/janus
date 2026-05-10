//! Objective add-note command

use serde_json::json;

use crate::cli::OutputOptions;
use crate::commands::CommandOutput;
use crate::error::{JanusError, Result};
use crate::objective::Objective;
use crate::store::get_or_init_store;
use crate::utils::iso_date;

/// Add a timestamped note to an objective
///
/// # Arguments
/// * `id` - Objective ID (full or partial)
/// * `text` - Note text
/// * `output` - Output options (JSON vs text)
pub async fn cmd_objective_add_note(id: &str, text: &str, output: OutputOptions) -> Result<()> {
    // Validate note text
    if text.trim().is_empty() {
        return Err(JanusError::EmptyNote);
    }

    const MAX_NOTE_LENGTH: usize = 10000;
    if text.len() > MAX_NOTE_LENGTH {
        return Err(JanusError::NoteTooLong {
            max: MAX_NOTE_LENGTH,
            actual: text.len(),
        });
    }

    let objective = Objective::find(id).await?;

    // Add the note (handles hooks + event logging internally)
    objective.add_note(text)?;

    // Refresh store
    if let Ok(store) = get_or_init_store().await {
        store.refresh_objective_in_store(&objective.id).await;
    }

    let timestamp = iso_date();

    CommandOutput::new(json!({
        "id": objective.id,
        "action": "note_added",
        "timestamp": timestamp,
        "note": text,
    }))
    .with_text(format!("Note added to {}", objective.id))
    .print(output)
}
