use owo_colors::OwoColorize;
use serde::Serialize;
use serde_json::json;

use crate::cli::OutputOptions;
use crate::commands::CommandOutput;
use crate::error::Result;
use crate::next::{InclusionReason, NextWorkFinder, WorkItem};
use crate::status::is_dependency_satisfied;
use crate::ticket::build_ticket_map;
use crate::types::TicketData;

/// JSON output structure for a work item
#[derive(Serialize)]
struct WorkItemJson {
    id: String,
    priority: u8,
    status: String,
    title: String,
    reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocks: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocked_by: Option<Vec<String>>,
}

/// Show next ticket(s) to work on (dependency-aware)
pub async fn cmd_next(limit: usize, output: OutputOptions) -> Result<()> {
    let ticket_map = build_ticket_map().await?;

    if ticket_map.is_empty() {
        return CommandOutput::new(json!([]))
            .with_text("No tickets found.")
            .print(output);
    }

    // Check if all tickets are complete or cancelled
    let all_complete = ticket_map.values().all(|t| {
        matches!(
            t.status,
            Some(crate::types::TicketStatus::Complete)
                | Some(crate::types::TicketStatus::Cancelled)
        )
    });

    if all_complete {
        return CommandOutput::new(json!([]))
            .with_text("All tickets are complete. Nothing to work on.")
            .print(output);
    }

    let finder = NextWorkFinder::new(&ticket_map);
    let work_items = finder.get_next_work(limit);

    if work_items.is_empty() {
        return CommandOutput::new(json!([]))
            .with_text("No tickets ready to work on.")
            .print(output);
    }

    // Build JSON output
    let json_items: Vec<WorkItemJson> = work_items
        .iter()
        .map(|item| work_item_to_json(item, &ticket_map))
        .collect();

    // Build text output
    let text_output = format_table(&work_items);

    CommandOutput::new(json!(json_items))
        .with_text(text_output)
        .print(output)
}

/// Convert a WorkItem to JSON representation
fn work_item_to_json(
    item: &WorkItem,
    ticket_map: &std::collections::HashMap<String, crate::types::TicketMetadata>,
) -> WorkItemJson {
    let priority = item.metadata.priority_num();
    let status = format_status(&item.reason);
    let title = item.metadata.title.clone().unwrap_or_default();
    let reason = format_reason(&item.reason);

    let blocks = item.blocks.clone();

    // For blocked tickets, include the list of unsatisfied dependencies
    let blocked_by = if matches!(item.reason, InclusionReason::TargetBlocked) {
        let deps: Vec<String> = item
            .metadata
            .deps
            .iter()
            .filter(|dep_id| !is_dependency_satisfied(dep_id.as_ref(), ticket_map))
            .map(|dep_id| dep_id.to_string())
            .collect();
        if deps.is_empty() { None } else { Some(deps) }
    } else {
        None
    };

    WorkItemJson {
        id: item.ticket_id.clone(),
        priority,
        status,
        title,
        reason,
        blocks,
        blocked_by,
    }
}

/// Format the status string based on inclusion reason
fn format_status(reason: &InclusionReason) -> String {
    match reason {
        InclusionReason::Ready => "ready".to_string(),
        InclusionReason::Blocking(_) => "ready".to_string(),
        InclusionReason::TargetBlocked => "blocked".to_string(),
    }
}

/// Format the reason string for display
fn format_reason(reason: &InclusionReason) -> String {
    match reason {
        InclusionReason::Ready => "ready".to_string(),
        InclusionReason::Blocking(_target) => "blocking".to_string(),
        InclusionReason::TargetBlocked => "target".to_string(),
    }
}

/// Format work items as a formatted table string
fn format_table(items: &[WorkItem]) -> String {
    // Define column widths
    const ID_WIDTH: usize = 10;
    const PRIORITY_WIDTH: usize = 8;
    const STATUS_WIDTH: usize = 10;
    const TITLE_WIDTH: usize = 30;

    let mut lines: Vec<String> = Vec::new();

    // Header line
    lines.push(format!(
        "{:<ID_WIDTH$}  {:<PRIORITY_WIDTH$}  {:<STATUS_WIDTH$}  {:<TITLE_WIDTH$}  Reason",
        "ID", "Priority", "Status", "Title"
    ));
    // Separator line
    lines.push(format!(
        "{:<ID_WIDTH$}  {:<PRIORITY_WIDTH$}  {:<STATUS_WIDTH$}  {:<TITLE_WIDTH$}  ─────────────────",
        "────────", "────────", "───────", "─────────────────────────────"
    ));

    for item in items {
        let id = &item.ticket_id;
        let priority = format!("P{}", item.metadata.priority_num());
        let status = format_status(&item.reason);
        let title = item
            .metadata
            .title
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(TITLE_WIDTH)
            .collect::<String>();
        let reason = format_reason_text(item);

        // Apply colors
        let colored_id = id.cyan().to_string();
        let colored_priority = match item.metadata.priority_num() {
            0 => priority.red().to_string(),
            1 => priority.yellow().to_string(),
            _ => priority,
        };
        let colored_status = match status.as_str() {
            "ready" => status.green().to_string(),
            "blocked" => status.red().to_string(),
            _ => status,
        };

        lines.push(format!(
            "{colored_id:<ID_WIDTH$}  {colored_priority:<PRIORITY_WIDTH$}  {colored_status:<STATUS_WIDTH$}  {title:<TITLE_WIDTH$}  {reason}"
        ));
    }

    lines.join("\n")
}

/// Format the reason text for table display
fn format_reason_text(item: &WorkItem) -> String {
    match &item.reason {
        InclusionReason::Ready => "ready".to_string(),
        InclusionReason::Blocking(target) => format!("blocks {}", target.cyan()),
        InclusionReason::TargetBlocked => {
            let dep_count = item.metadata.deps.len();
            if dep_count == 1 {
                "target (1 dep)".to_string()
            } else {
                format!("target ({dep_count} deps)")
            }
        }
    }
}
