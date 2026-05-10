//! Objective add-criterion command

use serde_json::json;

use crate::cli::OutputOptions;
use crate::commands::CommandOutput;
use crate::error::Result;
use crate::objective::Objective;
use crate::store::get_or_init_store;

/// Add an acceptance criterion to an objective
///
/// # Arguments
/// * `id` - Objective ID (full or partial)
/// * `criterion` - Criterion text (will be sanitized for safe markdown insertion)
/// * `output` - Output options (JSON vs text)
pub async fn cmd_objective_add_criterion(
    id: &str,
    criterion: &str,
    output: OutputOptions,
) -> Result<()> {
    let objective = Objective::find(id).await?;

    // add_criterion handles validation and sanitization internally
    objective.add_criterion(criterion)?;

    // Refresh store
    if let Ok(store) = get_or_init_store().await {
        store.refresh_objective_in_store(&objective.id).await;
    }

    CommandOutput::new(json!({
        "id": objective.id,
        "action": "criterion_added",
        "criterion": criterion,
    }))
    .with_text(format!("Criterion added to {}", objective.id))
    .print(output)
}
