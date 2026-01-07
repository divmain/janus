use std::io::Write;
use std::process::{Command, Stdio};

use crate::error::Result;
use crate::ticket::get_all_tickets;
use crate::types::TicketMetadata;

/// Convert TicketMetadata to a serde_json::Value for jq processing
fn ticket_to_json(ticket: &TicketMetadata) -> serde_json::Value {
    serde_json::json!({
        "id": ticket.id,
        "title": ticket.title,
        "status": ticket.status.map(|s| s.to_string()),
        "deps": ticket.deps,
        "links": ticket.links,
        "created": ticket.created,
        "type": ticket.ticket_type.map(|t| t.to_string()),
        "priority": ticket.priority.map(|p| p.to_string()),
        "assignee": ticket.assignee,
        "external-ref": ticket.external_ref,
        "parent": ticket.parent,
        "filePath": ticket.file_path.as_ref().map(|p| p.to_string_lossy().to_string()),
    })
}

/// Output tickets as JSON, optionally filtered with jq syntax
pub async fn cmd_query(filter: Option<&str>) -> Result<()> {
    let tickets = get_all_tickets().await;

    // Build JSON lines output
    let output: String = tickets
        .iter()
        .map(|t| serde_json::to_string(&ticket_to_json(t)).unwrap())
        .collect::<Vec<_>>()
        .join("\n");

    if let Some(filter_expr) = filter {
        // Spawn jq to process the filter
        let filter_str = format!("select({})", filter_expr);

        let mut child = Command::new("jq")
            .args(["-c", &filter_str])
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(output.as_bytes())?;
        }

        child.wait()?;
    } else {
        // No filter, output all tickets as JSON lines
        println!("{}", output);
    }

    Ok(())
}
