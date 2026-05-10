//! Objective edit command

use serde_json::json;

use crate::cli::OutputOptions;
use crate::commands::{CommandOutput, open_in_editor_for_entity};
use crate::error::Result;
use crate::objective::Objective;

/// Open an objective in the default editor
///
/// # Arguments
/// * `id` - Objective ID (full or partial)
/// * `output` - Output options (JSON vs text)
pub async fn cmd_objective_edit(id: &str, output: OutputOptions) -> Result<()> {
    let objective = Objective::find(id).await?;

    if output.json {
        return CommandOutput::new(json!({
            "id": objective.id,
            "file_path": objective.file_path.to_string_lossy(),
            "action": "edit",
        }))
        .print(output);
    }

    open_in_editor_for_entity("objective", &objective.file_path, output)
}
