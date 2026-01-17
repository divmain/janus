//! Plan list command

use owo_colors::OwoColorize;
use serde_json::json;

use super::format_status_badge;
use crate::commands::print_json;
use crate::error::Result;
use crate::plan::{compute_plan_status, get_all_plans_sync};
use crate::ticket::build_ticket_map;
use crate::types::TicketStatus;

/// List all plans
///
/// # Arguments
/// * `status_filter` - Optional status to filter by
/// * `output_json` - If true, output as JSON
pub async fn cmd_plan_ls(status_filter: Option<&str>, output_json: bool) -> Result<()> {
    let plans = get_all_plans_sync();
    let ticket_map = build_ticket_map().await;

    // Parse the status filter if provided
    let filter_status: Option<TicketStatus> = status_filter.and_then(|s| s.parse().ok());

    // Collect filtered plans with their statuses
    let mut filtered_plans: Vec<(
        &crate::plan::types::PlanMetadata,
        crate::plan::types::PlanStatus,
    )> = Vec::new();

    for metadata in &plans {
        let plan_status = compute_plan_status(metadata, &ticket_map);

        // Apply status filter
        if let Some(ref filter) = filter_status
            && plan_status.status != *filter
        {
            continue;
        }

        filtered_plans.push((metadata, plan_status));
    }

    // Handle JSON output
    if output_json {
        let json_plans: Vec<serde_json::Value> = filtered_plans
            .iter()
            .map(|(metadata, plan_status)| {
                json!({
                    "id": metadata.id,
                    "uuid": metadata.uuid,
                    "title": metadata.title,
                    "created": metadata.created,
                    "status": plan_status.status.to_string(),
                    "completed_count": plan_status.completed_count,
                    "total_count": plan_status.total_count,
                    "progress_percent": plan_status.progress_percent(),
                    "is_phased": metadata.is_phased(),
                })
            })
            .collect();

        print_json(&serde_json::Value::Array(json_plans))?;
        return Ok(());
    }

    // Default text output
    for (metadata, plan_status) in &filtered_plans {
        let id = metadata.id.as_deref().unwrap_or("???");
        let title = metadata.title.as_deref().unwrap_or("");
        let status_badge = format_status_badge(plan_status.status);
        let progress = format!(
            "{}/{}",
            plan_status.completed_count, plan_status.total_count
        );

        println!(
            "{:12} {} {:>5}  {}",
            id.cyan(),
            status_badge,
            progress.dimmed(),
            title
        );
    }

    Ok(())
}
