//! Plan edit command

use serde_json::json;

use crate::commands::{CommandOutput, open_in_editor_for_entity};
use crate::error::Result;
use crate::plan::Plan;

/// Open a plan in the default editor
///
/// # Arguments
/// * `id` - The plan ID (can be partial)
/// * `output_json` - If true, output result as JSON
pub async fn cmd_plan_edit(id: &str, output_json: bool) -> Result<()> {
    let plan = Plan::find(id).await?;

    if output_json {
        return CommandOutput::new(json!({
            "id": plan.id,
            "file_path": plan.file_path.to_string_lossy(),
            "action": "edit",
        }))
        .print(output_json);
    }

    open_in_editor_for_entity("plan", &plan.file_path, output_json)
}
