use serde_json::json;

use super::CommandOutput;
use crate::error::{JanusError, Result};
use crate::ticket::Ticket;

/// Add symmetric links between tickets
pub async fn cmd_link_add(ids: &[String], output_json: bool) -> Result<()> {
    if ids.len() < 2 {
        return Err(JanusError::Other(
            "At least two ticket IDs are required".to_string(),
        ));
    }

    // Find all tickets first to validate they exist
    let mut tickets: Vec<Ticket> = Vec::new();
    for id in ids {
        tickets.push(Ticket::find(id).await?);
    }

    let mut added_count = 0;

    // Add links between all pairs
    for ticket in &tickets {
        for other in &tickets {
            if ticket.id != other.id && ticket.add_to_array_field("links", &other.id)? {
                added_count += 1;
            }
        }
    }

    let mut links_updated = serde_json::Map::new();
    for ticket in &tickets {
        let metadata = ticket.read()?;
        links_updated.insert(ticket.id.clone(), json!(metadata.links));
    }
    let ticket_ids: Vec<_> = tickets.iter().map(|t| t.id.clone()).collect();
    let num_tickets = tickets.len();

    let text = if added_count == 0 {
        "All links already exist".to_string()
    } else {
        format!(
            "Added {} link(s) between {} tickets",
            added_count, num_tickets
        )
    };

    CommandOutput::new(json!({
        "action": if added_count > 0 { "linked" } else { "already_linked" },
        "tickets": ticket_ids,
        "links_added": added_count,
        "links_updated": links_updated,
    }))
    .with_text(text)
    .print(output_json)
}

/// Remove symmetric links between two tickets
pub async fn cmd_link_remove(id1: &str, id2: &str, output_json: bool) -> Result<()> {
    let ticket1 = Ticket::find(id1).await?;
    let ticket2 = Ticket::find(id2).await?;

    let mut removed_count = 0;

    if ticket1.remove_from_array_field("links", &ticket2.id)? {
        removed_count += 1;
    }
    if ticket2.remove_from_array_field("links", &ticket1.id)? {
        removed_count += 1;
    }

    if removed_count == 0 {
        return Err(JanusError::Other("Link not found".to_string()));
    }

    let metadata1 = ticket1.read()?;
    let metadata2 = ticket2.read()?;
    let mut links_updated = serde_json::Map::new();
    links_updated.insert(ticket1.id.clone(), json!(metadata1.links));
    links_updated.insert(ticket2.id.clone(), json!(metadata2.links));

    CommandOutput::new(json!({
        "action": "unlinked",
        "tickets": [&ticket1.id, &ticket2.id],
        "links_updated": links_updated,
    }))
    .with_text(format!("Removed link: {} <-> {}", ticket1.id, ticket2.id))
    .print(output_json)
}
