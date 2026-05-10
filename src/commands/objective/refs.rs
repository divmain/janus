//! Objective ref management commands (add, del, reset)

use serde_json::json;

use crate::commands::{CommandOutput, OutputOptions};
use crate::error::Result;
use crate::objective::Objective;
use crate::store::get_or_init_store;

pub async fn cmd_objective_ref_add(
    id: &str,
    ref_id: &str,
    output: OutputOptions,
) -> Result<()> {
    let objective = Objective::find(id).await?;
    objective.add_ref(ref_id)?;

    if let Ok(store) = get_or_init_store().await {
        store.refresh_objective_in_store(&objective.id).await;
    }

    CommandOutput::new(json!({
        "id": objective.id,
        "ref_added": ref_id,
    }))
    .with_text(format!("Added reference '{}' to objective '{}'", ref_id, objective.id))
    .print(output)
}

pub async fn cmd_objective_ref_del(
    id: &str,
    ref_id: &str,
    output: OutputOptions,
) -> Result<()> {
    let objective = Objective::find(id).await?;
    objective.remove_ref(ref_id)?;

    if let Ok(store) = get_or_init_store().await {
        store.refresh_objective_in_store(&objective.id).await;
    }

    CommandOutput::new(json!({
        "id": objective.id,
        "ref_removed": ref_id,
    }))
    .with_text(format!("Removed reference '{}' from objective '{}'", ref_id, objective.id))
    .print(output)
}

pub async fn cmd_objective_ref_reset(
    id: &str,
    force: bool,
    output: OutputOptions,
) -> Result<()> {
    let objective = Objective::find(id).await?;
    let metadata = objective.read()?;

    if metadata.satisfied_by.is_empty() {
        return CommandOutput::new(json!({
            "id": objective.id,
            "refs_cleared": 0,
        }))
        .with_text(format!("Objective '{}' has no references to clear.", objective.id))
        .print(output);
    }

    if !force {
        let count = metadata.satisfied_by.len();
        let prompt = format!(
            "Remove all {} reference(s) from objective '{}' ?",
            count, objective.id
        );
        if !crate::commands::interactive::confirm(&prompt)? {
            println!("Aborted.");
            return Ok(());
        }
    }

    let cleared_count = metadata.satisfied_by.len();
    objective.reset_refs()?;

    if let Ok(store) = get_or_init_store().await {
        store.refresh_objective_in_store(&objective.id).await;
    }

    CommandOutput::new(json!({
        "id": objective.id,
        "refs_cleared": cleared_count,
    }))
    .with_text(format!(
        "Cleared {} reference(s) from objective '{}'.",
        cleared_count, objective.id
    ))
    .print(output)
}
