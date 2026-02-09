//! Plan creation command

use serde_json::json;

use crate::commands::CommandOutput;
use crate::error::Result;
use crate::events::log_plan_created;
use crate::hooks::{run_post_hooks, run_pre_hooks, HookEvent};
use crate::plan::parser::serialize_plan;
use crate::plan::types::{Phase, PlanMetadata, PlanSection, TicketsSection};
use crate::plan::{ensure_plans_dir, generate_plan_id, Plan};

use crate::utils::{generate_uuid, iso_date};

/// Create a new plan
///
/// # Arguments
/// * `title` - The plan title
/// * `phases` - Optional list of initial phase names (creates a phased plan if provided)
/// * `output_json` - If true, output result as JSON
pub fn cmd_plan_create(title: &str, phases: &[String], output_json: bool) -> Result<()> {
    ensure_plans_dir()?;

    let id = generate_plan_id()?;
    let uuid = generate_uuid();
    let now = iso_date();

    // Build the plan metadata
    let mut metadata = PlanMetadata {
        id: Some(id.clone()),
        uuid: Some(uuid.clone()),
        created: Some(now.clone()),
        title: Some(title.to_string()),
        description: None,
        acceptance_criteria: Vec::new(),
        acceptance_criteria_raw: None,
        acceptance_criteria_extra: Vec::new(),
        sections: Vec::new(),
        file_path: None,
    };

    // Add phases if provided, otherwise create a simple plan with a Tickets section
    if phases.is_empty() {
        // Simple plan: add an empty Tickets section
        metadata
            .sections
            .push(PlanSection::Tickets(TicketsSection::new(Vec::new())));
    } else {
        // Phased plan: add phases with numbers
        for (i, phase_name) in phases.iter().enumerate() {
            let phase = Phase::new((i + 1).to_string(), phase_name.clone());
            metadata.sections.push(PlanSection::Phase(phase));
        }
    }

    // Serialize and write the plan
    let content = serialize_plan(&metadata);
    let plan = Plan::with_id(&id)?;

    // Build hook context for plan creation
    let context = plan.hook_context();

    // Run pre-write hook (can abort)
    run_pre_hooks(HookEvent::PreWrite, &context)?;

    // Write without internal hooks (we handle them here with PlanCreated instead of PlanUpdated)
    plan.write_without_hooks(&content)?;

    // Run post-write hooks (fire-and-forget)
    run_post_hooks(HookEvent::PostWrite, &context);
    run_post_hooks(HookEvent::PlanCreated, &context);

    // Log the event
    log_plan_created(&id, title, !phases.is_empty(), phases);

    CommandOutput::new(json!({
        "id": id,
        "uuid": uuid,
        "title": title,
        "created": now,
        "is_phased": !phases.is_empty(),
        "phases": phases,
    }))
    .with_text(&id)
    .print(output_json)
}
