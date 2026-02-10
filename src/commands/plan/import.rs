//! Plan import commands

use std::fs;
use std::io::Read;

use owo_colors::OwoColorize;
use serde_json::json;

use crate::commands::CommandOutput;
use crate::error::{JanusError, Result};
use crate::hooks::{HookEvent, run_post_hooks, run_pre_hooks};
use crate::plan::types::{FreeFormSection, Phase, PlanMetadata, PlanSection};
use crate::plan::{
    ImportablePlan, Plan, ensure_plans_dir, generate_plan_id, get_all_plans, parse_importable_plan,
};

use crate::types::{TicketPriority, TicketType};
use crate::utils::{generate_uuid, iso_date};

/// The Plan Format Specification document.
///
/// This constant contains the full documentation for the importable plan format.
/// It is displayed by `janus plan import-spec`.
pub const PLAN_FORMAT_SPECIFICATION: &str = include_str!("../../../docs/plan-import-format.md");

/// Show the importable plan format specification
///
/// Prints the Plan Format Specification document to stdout.
pub fn cmd_show_import_spec() -> Result<()> {
    println!("{PLAN_FORMAT_SPECIFICATION}");
    Ok(())
}

/// Check if a plan with the given title already exists
///
/// # Arguments
/// * `title` - The title to check
///
/// # Returns
/// `Ok(())` if no duplicate exists, `Err(DuplicatePlanTitle)` if one does.
async fn check_duplicate_plan_title(title: &str) -> Result<()> {
    let result = get_all_plans().await?;

    for plan in result.items {
        if let Some(ref existing_title) = plan.title
            && existing_title.eq_ignore_ascii_case(title)
        {
            let plan_id = plan
                .id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            return Err(JanusError::DuplicatePlanTitle(title.to_string(), plan_id));
        }
    }

    Ok(())
}

/// Format the dry-run import summary as a string
///
/// # Arguments
/// * `plan` - The parsed importable plan
fn format_import_summary(plan: &ImportablePlan) -> String {
    let mut out = String::new();

    out.push('\n');
    out.push_str(&format!("{}\n", "Import Summary".bold()));
    out.push_str(&format!("{}\n", "==============".bold()));
    out.push('\n');

    // Title
    out.push_str(&format!("{}: {}\n", "Title".bold(), plan.title));

    // Description (truncated if long)
    if let Some(ref desc) = plan.description {
        let desc_preview = if desc.chars().count() > 200 {
            let truncated: String = desc.chars().take(200).collect();
            format!("{truncated}...")
        } else {
            desc.clone()
        };
        out.push_str(&format!("{}: {}\n", "Description".bold(), desc_preview));
    }

    // Acceptance criteria
    if !plan.acceptance_criteria.is_empty() {
        out.push('\n');
        out.push_str(&format!(
            "{}: {} items\n",
            "Acceptance Criteria".bold(),
            plan.acceptance_criteria.len()
        ));
        for criterion in &plan.acceptance_criteria {
            out.push_str(&format!("  - {criterion}\n"));
        }
    }

    // Plan structure
    out.push('\n');
    out.push_str(&format!("{}: {}\n", "Phases".bold(), plan.phases.len()));
    out.push_str(&format!("{}: {}\n", "Tasks".bold(), plan.task_count()));
    out.push('\n');

    for phase in &plan.phases {
        let phase_header = if phase.name.is_empty() {
            format!("Phase {}", phase.number)
        } else {
            format!("Phase {}: {}", phase.number, phase.name)
        };
        out.push_str(&format!("{}\n", phase_header.cyan()));

        for task in &phase.tasks {
            let marker = if task.is_complete { "[x]" } else { "[ ]" };
            out.push_str(&format!("  {} {}\n", marker.dimmed(), task.title));
        }
    }

    // Summary of what would be created
    out.push('\n');
    out.push_str(&format!("{}\n", "Would create:".bold()));
    out.push_str("  - 1 plan\n");

    let new_count = plan.all_tasks().iter().filter(|t| !t.is_complete).count();
    let complete_count = plan.all_tasks().iter().filter(|t| t.is_complete).count();

    if complete_count > 0 {
        out.push_str(&format!(
            "  - {} tickets ({} new, {} complete)\n",
            plan.task_count(),
            new_count,
            complete_count
        ));
    } else {
        out.push_str(&format!(
            "  - {} tickets (status: new)\n",
            plan.task_count()
        ));
    }

    if !plan.acceptance_criteria.is_empty() {
        out.push_str("  - 1 verification ticket (from acceptance criteria)\n");
    }

    out.push('\n');
    out.push_str("Run without --dry-run to import.");
    out
}

/// Create a ticket from an ImportableTask
///
/// Returns (ticket_id, file_path) on success.
fn create_ticket_from_task(
    task: &crate::plan::ImportableTask,
    ticket_type: TicketType,
    prefix: Option<&str>,
) -> Result<String> {
    let status = if task.is_complete { "complete" } else { "new" };

    let (id, _file_path) = crate::ticket::TicketBuilder::new(&task.title)
        .description(task.body.as_deref())
        .prefix(prefix)
        .ticket_type_enum(ticket_type)
        .status(status)
        .priority_enum(TicketPriority::default())
        .run_hooks(true)
        .build()?;

    Ok(id)
}

/// Create a verification ticket for acceptance criteria
fn create_verification_ticket(
    criteria: &[String],
    ticket_type: TicketType,
    prefix: Option<&str>,
) -> Result<String> {
    let mut body = "Verify that all acceptance criteria have been met:\n\n".to_string();
    for criterion in criteria {
        body.push_str(&format!("- [ ] {criterion}\n"));
    }

    let (id, _file_path) = crate::ticket::TicketBuilder::new("Verify Acceptance Criteria")
        .description(Some(body))
        .prefix(prefix)
        .ticket_type_enum(ticket_type)
        .priority_enum(TicketPriority::default())
        .run_hooks(true)
        .build()?;

    Ok(id)
}

/// Import a plan from a markdown file
///
/// # Arguments
/// * `input` - File path or "-" for stdin
/// * `dry_run` - If true, validate and show summary without creating anything
/// * `title_override` - Override the extracted title
/// * `ticket_type` - Type for created tickets (default: task)
/// * `prefix` - Custom prefix for ticket IDs
/// * `output_json` - If true, output result as JSON
pub async fn cmd_plan_import(
    input: &str,
    dry_run: bool,
    title_override: Option<&str>,
    ticket_type: TicketType,
    prefix: Option<&str>,
    output_json: bool,
) -> Result<()> {
    // 1. Read content from file or stdin
    let content = if input == "-" {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        buffer
    } else {
        fs::read_to_string(input).map_err(|e| {
            JanusError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to read plan import file at {input}: {e}"),
            ))
        })?
    };

    // 2. Parse the importable plan
    let mut plan = parse_importable_plan(&content)?;

    // 3. Apply title override if provided
    if let Some(title) = title_override {
        plan.title = title.to_string();
    }

    // 4. Check for duplicate plan title
    check_duplicate_plan_title(&plan.title).await?;

    // 5. If dry-run, print summary and return
    if dry_run {
        let new_count = plan.all_tasks().iter().filter(|t| !t.is_complete).count();
        let complete_count = plan.all_tasks().iter().filter(|t| t.is_complete).count();

        return CommandOutput::new(json!({
            "dry_run": true,
            "valid": true,
            "title": plan.title,
            "description": plan.description,
            "acceptance_criteria": plan.acceptance_criteria,
            "acceptance_criteria_count": plan.acceptance_criteria.len(),
            "is_phased": plan.is_phased(),
            "phase_count": plan.phases.len(),
            "task_count": plan.task_count(),
            "phases": plan.phases.iter().map(|p| json!({
                "number": p.number,
                "name": p.name,
                "tasks": p.tasks.iter().map(|t| json!({
                    "title": t.title,
                    "is_complete": t.is_complete,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "would_create": {
                "plans": 1,
                "tickets": {
                    "total": plan.task_count() + if !plan.acceptance_criteria.is_empty() { 1 } else { 0 },
                    "new": new_count,
                    "complete": complete_count,
                    "verification": !plan.acceptance_criteria.is_empty(),
                }
            }
        }))
        .with_text(format_import_summary(&plan))
        .print(output_json);
    }

    // 6. Create all tickets
    ensure_plans_dir()?;

    let mut created_ticket_ids: Vec<String> = Vec::new();

    // Create tickets for each phase
    for phase in &plan.phases {
        for task in &phase.tasks {
            let ticket_id = create_ticket_from_task(task, ticket_type, prefix)?;
            created_ticket_ids.push(ticket_id);
        }
    }

    // 7. Create verification ticket if acceptance criteria exist
    let verification_ticket_id = if !plan.acceptance_criteria.is_empty() {
        Some(create_verification_ticket(
            &plan.acceptance_criteria,
            ticket_type,
            prefix,
        )?)
    } else {
        None
    };

    // 8. Generate plan metadata
    let plan_id = generate_plan_id()?;
    let uuid = generate_uuid();
    let now = iso_date();

    let mut metadata = PlanMetadata {
        id: Some(crate::types::PlanId::new_unchecked(plan_id.clone())),
        uuid: Some(uuid.clone()),
        created: Some(crate::types::CreatedAt::new_unchecked(now.clone())),
        title: Some(plan.title.clone()),
        description: plan.description.clone(),
        acceptance_criteria: plan.acceptance_criteria.clone(),
        acceptance_criteria_raw: None,
        acceptance_criteria_extra: Vec::new(),
        sections: Vec::new(),
        file_path: None,
        extra_frontmatter: None,
    };

    // 9. Include design section as a free-form section if present
    if let Some(ref design) = plan.design {
        metadata
            .sections
            .push(PlanSection::FreeForm(FreeFormSection::new(
                "Design",
                design.clone(),
            )));
    }

    // 10. Include implementation preamble as a free-form section if present
    if let Some(ref preamble) = plan.implementation_preamble {
        metadata
            .sections
            .push(PlanSection::FreeForm(FreeFormSection::new(
                "Implementation Overview",
                preamble.clone(),
            )));
    }

    // 11. Build sections with ticket IDs
    let mut ticket_idx = 0;
    for import_phase in &plan.phases {
        let mut phase = Phase::new(import_phase.number.clone(), import_phase.name.clone());
        phase.description = import_phase.description.clone();

        // Assign ticket IDs to this phase
        for _ in &import_phase.tasks {
            phase.tickets.push(created_ticket_ids[ticket_idx].clone());
            ticket_idx += 1;
        }

        // Add verification ticket to the last phase if it exists
        let is_last_phase = plan
            .phases
            .last()
            .map(|p| p.number == import_phase.number)
            .unwrap_or(false);

        if is_last_phase && let Some(ref v_id) = verification_ticket_id {
            phase.tickets.push(v_id.clone());
        }

        metadata.sections.push(PlanSection::Phase(phase));
    }

    // 12. Write plan with exactly-once hook semantics:
    //     PreWrite -> write -> PostWrite -> PlanCreated
    let plan_handle = Plan::with_id(&plan_id)?;
    let context = plan_handle.hook_context();

    // Pre-write hook can abort the import
    run_pre_hooks(HookEvent::PreWrite, &context)?;

    // Write without internal hooks — we manage the full lifecycle here
    // to emit PlanCreated (not PlanUpdated which write() would emit)
    plan_handle.write_metadata_without_hooks(&metadata)?;

    // Post-write hooks (fire-and-forget)
    run_post_hooks(HookEvent::PostWrite, &context);
    run_post_hooks(HookEvent::PlanCreated, &context);

    // 13. Output result
    let tickets_created: Vec<serde_json::Value> = created_ticket_ids
        .iter()
        .map(|id| json!({ "id": id }))
        .collect();

    CommandOutput::new(json!({
        "id": plan_id,
        "uuid": uuid,
        "title": plan.title,
        "created": now,
        "is_phased": plan.is_phased(),
        "tickets_created": tickets_created,
        "verification_ticket": verification_ticket_id,
    }))
    .with_text(&plan_id)
    .print(output_json)
}
