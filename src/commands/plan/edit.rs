//! Plan edit command

use serde_json::json;

use crate::cli::OutputOptions;
use crate::commands::{CommandOutput, open_in_editor_for_entity};
use crate::error::Result;
use crate::plan::Plan;

/// Open a plan in the default editor
///
/// # Arguments
/// * `id` - The plan ID (can be partial)
/// * `output` - If true, output result as JSON
pub async fn cmd_plan_edit(id: &str, output: OutputOptions) -> Result<()> {
    let plan = Plan::find(id).await?;

    if output.json {
        return CommandOutput::new(json!({
            "id": plan.id,
            "file_path": plan.file_path.to_string_lossy(),
            "action": "edit",
        }))
        .print(output);
    }

    open_in_editor_for_entity("plan", &plan.file_path, output)
}
