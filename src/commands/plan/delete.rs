//! Plan delete and rename commands

use serde_json::json;

use crate::cli::OutputOptions;
use crate::commands::{CommandOutput, interactive};
use crate::error::Result;
use crate::plan::Plan;
use crate::utils::is_stdin_tty;

/// Delete a plan
///
/// # Arguments
/// * `id` - The plan ID (can be partial)
/// * `force` - Skip confirmation prompt
/// * `output_json` - If true, output result as JSON
pub async fn cmd_plan_delete(id: &str, force: bool, output: OutputOptions) -> Result<()> {
    let plan = Plan::find(id).await?;

    if !force {
        if output.json || !is_stdin_tty() {
            return Err(crate::error::JanusError::ConfirmationRequired(
                "Plan deletion requires --force flag in non-interactive contexts. Use --force to confirm deletion.".to_string()
            ));
        }
        if !interactive::confirm(&format!("Delete plan {}", plan.id))? {
            println!("Cancelled");
            return Ok(());
        }
    }

    let plan_id = plan.id.clone();
    plan.delete()?;

    CommandOutput::new(json!({
        "plan_id": plan_id,
        "action": "deleted",
        "success": true,
    }))
    .with_text(format!("Deleted plan {plan_id}"))
    .print(output)
}

/// Rename a plan (update its title)
///
/// # Arguments
/// * `id` - The plan ID (can be partial)
/// * `new_title` - The new title
/// * `output_json` - If true, output result as JSON
pub async fn cmd_plan_rename(id: &str, new_title: &str, output: OutputOptions) -> Result<()> {
    let plan = Plan::find(id).await?;
    let mut metadata = plan.read()?;

    let old_title = metadata.title.clone().unwrap_or_default();
    metadata.title = Some(new_title.to_string());

    // Write updated plan
    plan.write_metadata(&metadata)?;

    CommandOutput::new(json!({
        "plan_id": plan.id,
        "action": "renamed",
        "old_title": old_title,
        "new_title": new_title,
    }))
    .with_text(format!(
        "Renamed plan {} from '{}' to '{}'",
        plan.id, old_title, new_title
    ))
    .print(output)
}
