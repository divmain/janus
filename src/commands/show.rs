use owo_colors::OwoColorize;
use serde_json::json;

use super::CommandOutput;
use crate::commands::format_ticket_bullet;
use crate::error::Result;
use crate::ticket::{Ticket, build_ticket_map, get_children_count};
use crate::types::{TicketMetadata, TicketStatus};

/// Display a ticket with its relationships
pub async fn cmd_show(id: &str, output_json: bool) -> Result<()> {
    let ticket = Ticket::find(id).await?;
    let content = ticket.read_content()?;
    let metadata = ticket.read()?;
    let ticket_map = build_ticket_map().await;

    let mut blockers: Vec<&TicketMetadata> = Vec::new();
    let mut blocking: Vec<&TicketMetadata> = Vec::new();
    let mut children: Vec<&TicketMetadata> = Vec::new();

    for (other_id, other) in &ticket_map {
        if other_id == &ticket.id {
            continue;
        }

        // Check if this is a child of the current ticket
        if other.parent.as_ref() == Some(&ticket.id) {
            children.push(other);
        }

        // Check if this ticket is blocked by the current ticket
        if other.deps.contains(&ticket.id) && other.status != Some(TicketStatus::Complete) {
            blocking.push(other);
        }
    }

    // Find blockers (deps that are not complete)
    for dep_id in &metadata.deps {
        if let Some(dep) = ticket_map.get(dep_id)
            && dep.status != Some(TicketStatus::Complete)
        {
            blockers.push(dep);
        }
    }

    // Get count of tickets spawned from this ticket
    let spawned_count = get_children_count(&ticket.id).await;

    // Build JSON data (needed for both output formats)
    let blockers_json: Vec<_> = blockers
        .iter()
        .copied()
        .map(super::ticket_minimal_json)
        .collect();

    let blocking_json: Vec<_> = blocking
        .iter()
        .copied()
        .map(super::ticket_minimal_json)
        .collect();

    let children_json: Vec<_> = children
        .iter()
        .copied()
        .map(super::ticket_minimal_json)
        .collect();

    let linked_json: Vec<_> = metadata
        .links
        .iter()
        .filter_map(|link_id| ticket_map.get(link_id))
        .map(super::ticket_minimal_json)
        .collect();

    let json_output = json!({
        "id": metadata.id,
        "uuid": metadata.uuid,
        "title": metadata.title,
        "status": metadata.status.map(|s| s.to_string()),
        "type": metadata.ticket_type.map(|t| t.to_string()),
        "priority": metadata.priority.map(|p| p.as_num()),
        "created": metadata.created,
        "deps": metadata.deps,
        "links": metadata.links,
        "parent": metadata.parent,
        "external_ref": metadata.external_ref,
        "remote": metadata.remote,
        "file_path": metadata.file_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        "completion_summary": metadata.completion_summary,
        "blockers": blockers_json,
        "blocking": blocking_json,
        "children": children_json,
        "linked": linked_json,
        "children_count": spawned_count,
    });

    // Build text output
    let text_output = {
        let mut output = content;

        // Print completion summary if ticket is complete and has one
        if metadata.status == Some(TicketStatus::Complete)
            && let Some(ref summary) = metadata.completion_summary
        {
            output.push('\n');
            output.push_str(&format!("{}", "Completion Summary:".green().bold()));
            for line in summary.lines() {
                output.push_str(&format!("\n  {}", line.dimmed()));
            }
        }

        // Print sections
        output.push_str(&format_section("Blockers", &blockers));
        output.push_str(&format_section("Blocking", &blocking));
        output.push_str(&format_section("Children", &children));

        // Print linked tickets
        if !metadata.links.is_empty() {
            output.push_str("\n\n## Linked");
            for link_id in &metadata.links {
                if let Some(linked) = ticket_map.get(link_id) {
                    output.push_str(&format!("\n{}", format_ticket_bullet(linked)));
                }
            }
        }

        // Print spawned children count (only if > 0)
        if spawned_count > 0 {
            output.push_str(&format!(
                "\n\n{} {} spawned from this ticket",
                "Children:".green().bold(),
                spawned_count
            ));
        }

        output
    };

    CommandOutput::new(json_output)
        .with_text(text_output)
        .print(output_json)
}

fn format_section(title: &str, items: &[&TicketMetadata]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut output = format!("\n\n## {}", title);
    for item in items {
        output.push_str(&format!("\n{}", format_ticket_bullet(item)));
    }
    output
}
