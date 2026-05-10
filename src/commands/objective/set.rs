//! Objective set command

use serde_json::json;

use crate::cli::OutputOptions;
use crate::commands::CommandOutput;
use crate::error::{JanusError, Result};
use crate::objective::Objective;
use crate::store::get_or_init_store;

/// Supported fields for the objective set command
const SUPPORTED_FIELDS: &[&str] = &["satisfied-by"];

/// Set a field on an objective
///
/// # Arguments
/// * `id` - Objective ID (full or partial)
/// * `field` - Field name (currently only "satisfied-by")
/// * `value` - New value (empty string to clear)
/// * `output` - Output options (JSON vs text)
pub async fn cmd_objective_set(
    id: &str,
    field: &str,
    value: &str,
    output: OutputOptions,
) -> Result<()> {
    // Validate field
    if !SUPPORTED_FIELDS.contains(&field) {
        return Err(JanusError::InvalidInput(format!(
            "Unsupported field '{field}'. Supported fields: {}",
            SUPPORTED_FIELDS.join(", ")
        )));
    }

    let objective = Objective::find(id).await?;

    // Get previous value for reporting
    let metadata = objective.read()?;
    let previous_value = match field {
        "satisfied-by" => metadata.satisfied_by.clone(),
        _ => None,
    };

    if value.is_empty() {
        // Clear the field by removing it from frontmatter
        let raw_content = objective.read_content()?;
        let new_content = crate::ticket::remove_field(&raw_content, field)?;
        objective.write(&new_content)?;

        crate::events::log_objective_field_updated(
            &objective.id,
            field,
            previous_value.as_deref(),
            None,
            None,
        );
    } else {
        // Update the field
        objective.update_field(field, value)?;
    }

    // Refresh store
    if let Ok(store) = get_or_init_store().await {
        store.refresh_objective_in_store(&objective.id).await;
    }

    let new_value_display = if value.is_empty() {
        "(cleared)".to_string()
    } else {
        value.to_string()
    };

    CommandOutput::new(json!({
        "id": objective.id,
        "action": "field_updated",
        "field": field,
        "previous": previous_value,
        "value": if value.is_empty() { None } else { Some(value) },
    }))
    .with_text(format!(
        "Updated {} on {}: {}",
        field, objective.id, new_value_display
    ))
    .print(output)
}
