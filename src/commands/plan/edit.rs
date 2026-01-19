//! Plan edit command

use serde_json::json;

use crate::commands::CommandOutput;
use crate::error::Result;
use crate::plan::Plan;
use crate::utils::{is_stdin_tty, open_in_editor};

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

    if is_stdin_tty() {
        open_in_editor(&plan.file_path)?;
    } else {
        // Non-interactive mode: just print the file path
        println!("Edit plan file: {}", plan.file_path.display());
    }

    Ok(())
}
