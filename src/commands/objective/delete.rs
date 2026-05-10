//! Objective delete command

use serde_json::json;

use crate::cli::OutputOptions;
use crate::commands::{CommandOutput, interactive};
use crate::error::Result;
use crate::objective::Objective;
use crate::store::get_or_init_store;
use crate::utils::is_stdin_tty;

/// Delete an objective
///
/// # Arguments
/// * `id` - Objective ID (full or partial)
/// * `yes` - Skip confirmation prompt
/// * `output` - Output options (JSON vs text)
pub async fn cmd_objective_delete(id: &str, yes: bool, output: OutputOptions) -> Result<()> {
    let objective = Objective::find(id).await?;

    if !yes {
        if output.json || !is_stdin_tty() {
            return Err(crate::error::JanusError::ConfirmationRequired(
                "Objective deletion requires -y/--yes flag in non-interactive contexts. Use -y to confirm deletion.".to_string()
            ));
        }
        if !interactive::confirm(&format!("Delete objective {}", objective.id))? {
            println!("Cancelled");
            return Ok(());
        }
    }

    let objective_id = objective.id.clone();

    // Delete the file (handles hooks + event logging internally)
    objective.delete()?;

    // Remove from store
    if let Ok(store) = get_or_init_store().await {
        store.remove_objective(&objective_id);
    }

    CommandOutput::new(json!({
        "id": objective_id,
        "action": "deleted",
        "success": true,
    }))
    .with_text(format!("Deleted objective {objective_id}"))
    .print(output)
}
