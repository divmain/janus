//! Objective creation command

use serde_json::json;

use crate::cli::OutputOptions;
use crate::commands::CommandOutput;
use crate::error::{JanusError, Result};
use crate::events::log_objective_created;
use crate::hooks::{HookEvent, run_post_hooks, run_pre_hooks};
use crate::objective::{Objective, ObjectiveBuilder};
use crate::store::get_or_init_store;

/// Create a new objective
///
/// # Arguments
/// * `title` - The objective title
/// * `description` - Optional description text
/// * `criteria` - Acceptance criteria (list of strings)
/// * `satisfied_by` - Optional ticket or plan ID that satisfies this objective
/// * `output` - Output options (JSON vs text)
pub async fn cmd_objective_create(
    title: &str,
    description: Option<&str>,
    criteria: &[String],
    satisfied_by: Option<&str>,
    output: OutputOptions,
) -> Result<()> {
    // Validate title
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err(JanusError::InvalidInput(
            "Objective title cannot be empty".to_string(),
        ));
    }
    if trimmed.len() > 200 {
        return Err(JanusError::InvalidInput(format!(
            "Objective title too long: {} characters (max: 200)",
            trimmed.len()
        )));
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(JanusError::InvalidInput(
            "Objective title contains invalid control characters".to_string(),
        ));
    }

    // Build the objective
    let mut builder = ObjectiveBuilder::new(trimmed);
    if let Some(desc) = description {
        builder = builder.description(desc);
    }
    if !criteria.is_empty() {
        builder = builder.acceptance_criteria(criteria.to_vec());
    }
    if let Some(ref_id) = satisfied_by {
        builder = builder.satisfied_by(ref_id);
    }

    let (id, content) = builder.build()?;

    // Create the objective handle and write
    let objective = Objective::with_id(&id)?;
    let context = objective.hook_context();

    // Run pre-write hook (can abort)
    run_pre_hooks(HookEvent::PreWrite, &context)?;

    // Write without internal hooks (we handle them here with ObjectiveCreated instead of ObjectiveUpdated)
    // Use write_raw via the public write method but we need to avoid double-hooking.
    // Since Objective::write() runs hooks internally, and we want to run ObjectiveCreated
    // instead of ObjectiveUpdated, we'll use the fs module directly like plan create does.
    crate::fs::ensure_parent_dir(&objective.file_path)?;
    crate::fs::write_file_atomic(&objective.file_path, &content)?;

    // Run post-write hooks
    run_post_hooks(HookEvent::PostWrite, &context);
    run_post_hooks(HookEvent::ObjectiveCreated, &context);

    // Refresh store
    if let Ok(store) = get_or_init_store().await {
        store.refresh_objective_in_store(&id).await;
    }

    // Log event
    log_objective_created(&id, trimmed, None);

    CommandOutput::new(json!({
        "id": id,
        "title": trimmed,
    }))
    .with_text(&id)
    .print(output)
}
