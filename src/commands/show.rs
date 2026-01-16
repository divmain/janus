use owo_colors::OwoColorize;
use serde_json::json;

use super::print_json;
use crate::commands::format_ticket_bullet;
use crate::error::Result;
use crate::ticket::{Ticket, build_ticket_map};
use crate::types::{TicketMetadata, TicketStatus};

/// Display a ticket with its relationships
pub async fn cmd_show(id: &str, output_json: bool) -> Result<()> {
    let ticket = Ticket::find_async(id).await?;
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

    if output_json {
        let blockers_json: Vec<_> = blockers
            .iter()
            .map(|t| {
                json!({
                    "id": t.id,
                    "title": t.title,
                    "status": t.status.map(|s| s.to_string()),
                })
            })
            .collect();

        let blocking_json: Vec<_> = blocking
            .iter()
            .map(|t| {
                json!({
                    "id": t.id,
                    "title": t.title,
                    "status": t.status.map(|s| s.to_string()),
                })
            })
            .collect();

        let children_json: Vec<_> = children
            .iter()
            .map(|t| {
                json!({
                    "id": t.id,
                    "title": t.title,
                    "status": t.status.map(|s| s.to_string()),
                })
            })
            .collect();

        let linked_json: Vec<_> = metadata
            .links
            .iter()
            .filter_map(|link_id| ticket_map.get(link_id))
            .map(|t| {
                json!({
                    "id": t.id,
                    "title": t.title,
                    "status": t.status.map(|s| s.to_string()),
                })
            })
            .collect();

        print_json(&json!({
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
        }))?;
        return Ok(());
    }

    // Print the raw content
    println!("{}", content);

    // Print completion summary if ticket is complete and has one
    // (This is separate from the raw content because we format it nicely)
    if metadata.status == Some(TicketStatus::Complete)
        && let Some(ref summary) = metadata.completion_summary
    {
        // Only print if the summary isn't already in the raw content
        // (The raw content contains the ## Completion Summary section)
        // We print a formatted version to highlight it
        println!();
        println!("{}", "Completion Summary:".green().bold());
        for line in summary.lines() {
            println!("  {}", line.dimmed());
        }
    }

    // Print sections
    print_section("Blockers", &blockers);
    print_section("Blocking", &blocking);
    print_section("Children", &children);

    // Print linked tickets
    if !metadata.links.is_empty() {
        println!("\n## Linked");
        for link_id in &metadata.links {
            if let Some(linked) = ticket_map.get(link_id) {
                println!("{}", format_ticket_bullet(linked));
            }
        }
    }

    Ok(())
}

fn print_section(title: &str, items: &[&TicketMetadata]) {
    if !items.is_empty() {
        println!("\n## {}", title);
        for item in items {
            println!("{}", format_ticket_bullet(item));
        }
    }
}
