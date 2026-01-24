//! Plan import commands

use std::fs;
use std::io::Read;

use owo_colors::OwoColorize;
use serde_json::json;

use crate::commands::{CommandOutput, print_json};
use crate::error::{JanusError, Result};
use crate::hooks::{HookEvent, run_post_hooks, run_pre_hooks};
use crate::plan::parser::serialize_plan;
use crate::plan::types::{Phase, PlanMetadata, PlanSection};
use crate::plan::{
    ImportablePlan, Plan, ensure_plans_dir, generate_plan_id, get_all_plans, parse_importable_plan,
};
use crate::storage::FileStorage;
use crate::types::TicketType;
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
    println!("{}", PLAN_FORMAT_SPECIFICATION);
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
    let existing_plans = get_all_plans().await;

    for plan in existing_plans {
        if let Some(ref existing_title) = plan.title
            && existing_title.eq_ignore_ascii_case(title)
        {
            let plan_id = plan.id.unwrap_or_else(|| "unknown".to_string());
            return Err(JanusError::DuplicatePlanTitle(title.to_string(), plan_id));
        }
    }

    Ok(())
}

/// Format and print the dry-run import summary
///
/// # Arguments
/// * `plan` - The parsed importable plan
fn print_import_summary(plan: &ImportablePlan) {
    println!();
    println!("{}", "Import Summary".bold());
    println!("{}", "==============".bold());
    println!();

    // Title
    println!("{}: {}", "Title".bold(), plan.title);

    // Description (truncated if long)
    if let Some(ref desc) = plan.description {
        let desc_preview = if desc.len() > 200 {
            format!("{}...", &desc[..200])
        } else {
            desc.clone()
        };
        println!("{}: {}", "Description".bold(), desc_preview);
    }

    // Acceptance criteria
    if !plan.acceptance_criteria.is_empty() {
        println!();
        println!(
            "{}: {} items",
            "Acceptance Criteria".bold(),
            plan.acceptance_criteria.len()
        );
        for criterion in &plan.acceptance_criteria {
            println!("  - {}", criterion);
        }
    }

    // Plan structure
    println!();
    println!("{}: {}", "Phases".bold(), plan.phases.len());
    println!("{}: {}", "Tasks".bold(), plan.task_count());
    println!();

    for phase in &plan.phases {
        let phase_header = if phase.name.is_empty() {
            format!("Phase {}", phase.number)
        } else {
            format!("Phase {}: {}", phase.number, phase.name)
        };
        println!("{}", phase_header.cyan());

        for task in &phase.tasks {
            let marker = if task.is_complete { "[x]" } else { "[ ]" };
            println!("  {} {}", marker.dimmed(), task.title);
        }
    }

    // Summary of what would be created
    println!();
    println!("{}", "Would create:".bold());
    println!("  - 1 plan");

    let new_count = plan.all_tasks().iter().filter(|t| !t.is_complete).count();
    let complete_count = plan.all_tasks().iter().filter(|t| t.is_complete).count();

    if complete_count > 0 {
        println!(
            "  - {} tickets ({} new, {} complete)",
            plan.task_count(),
            new_count,
            complete_count
        );
    } else {
        println!("  - {} tickets (status: new)", plan.task_count());
    }

    if !plan.acceptance_criteria.is_empty() {
        println!("  - 1 verification ticket (from acceptance criteria)");
    }

    println!();
    println!("Run without --dry-run to import.");
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
        .ticket_type(ticket_type.to_string())
        .status(status)
        .priority("2")
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
        body.push_str(&format!("- [ ] {}\n", criterion));
    }

    let (id, _file_path) = crate::ticket::TicketBuilder::new("Verify Acceptance Criteria")
        .description(Some(body))
        .prefix(prefix)
        .ticket_type(ticket_type.to_string())
        .priority("2")
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
                format!("Failed to read plan import file at {}: {}", input, e),
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
        if output_json {
            print_json(&json!({
                "dry_run": true,
                "title": plan.title,
                "description": plan.description,
                "acceptance_criteria_count": plan.acceptance_criteria.len(),
                "is_phased": plan.is_phased(),
                "phase_count": plan.phases.len(),
                "task_count": plan.task_count(),
                "would_create": {
                    "plans": 1,
                    "tickets": plan.task_count() + if !plan.acceptance_criteria.is_empty() { 1 } else { 0 },
                }
            }))?;
        } else {
            print_import_summary(&plan);
        }
        return Ok(());
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
    let plan_id = generate_plan_id();
    let uuid = generate_uuid();
    let now = iso_date();

    let mut metadata = PlanMetadata {
        id: Some(plan_id.clone()),
        uuid: Some(uuid.clone()),
        created: Some(now.clone()),
        title: Some(plan.title.clone()),
        description: plan.description.clone(),
        acceptance_criteria: plan.acceptance_criteria.clone(),
        sections: Vec::new(),
        file_path: None,
    };

    // 9. Build sections with ticket IDs
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

    // 10. Serialize and write plan
    let plan_content = serialize_plan(&metadata);
    let plan_handle = Plan::with_id(&plan_id);

    // Build hook context for plan creation
    let context = plan_handle.hook_context();

    // Run pre-write hook (can abort)
    run_pre_hooks(HookEvent::PreWrite, &context)?;

    // Write without internal hooks (we handle them here with PlanCreated instead of PlanUpdated)
    plan_handle.write_without_hooks(&plan_content)?;

    // Run post-write hooks (fire-and-forget)
    run_post_hooks(HookEvent::PostWrite, &context);
    run_post_hooks(HookEvent::PlanCreated, &context);

    // 11. Output result
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
