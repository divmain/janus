use serde_json::json;

use super::CommandOutput;
use crate::error::{JanusError, Result};
use crate::events::{log_link_added, log_link_removed};
use crate::ticket::Ticket;

/// Add symmetric links between tickets
pub async fn cmd_link_add(ids: &[String], output_json: bool) -> Result<()> {
    if ids.len() < 2 {
        return Err(JanusError::InsufficientTicketIds {
            expected: 2,
            provided: ids.len(),
        });
    }

    // Check for duplicate IDs (self-links)
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            if ids[i] == ids[j] {
                return Err(JanusError::SelfLink(ids[i].clone()));
            }
        }
    }

    // Find all tickets first to validate they exist
    let mut tickets: Vec<Ticket> = Vec::new();
    for id in ids {
        tickets.push(Ticket::find(id).await?);
    }

    let mut added_count = 0;
    let mut asymmetric_warnings: Vec<(String, String)> = Vec::new();

    // Add links between all pairs
    for ticket in &tickets {
        for other in &tickets {
            if ticket.id != other.id {
                let has_existing_link = ticket.has_in_array_field("links", &other.id)?;
                let other_has_link = other.has_in_array_field("links", &ticket.id)?;

                // Check for asymmetric link before adding
                if !has_existing_link && other_has_link {
                    // We're about to add A->B but B->A already exists
                    // This is expected behavior, but warn about the asymmetry
                    asymmetric_warnings.push((other.id.clone(), ticket.id.clone()));
                }

                if ticket.add_to_array_field("links", &other.id)? {
                    added_count += 1;
                    // Log the event for each link added
                    log_link_added(&ticket.id, &other.id);
                }
            }
        }
    }

    // Warn about asymmetric links (where only one direction existed before we added)
    for (from, to) in asymmetric_warnings {
        eprintln!(
            "Warning: Link from {} -> {} already existed but not vice versa. Link state is asymmetric.",
            from, to
        );
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
    let mut removed_1_to_2 = false;
    let mut removed_2_to_1 = false;

    if ticket1.remove_from_array_field("links", &ticket2.id)? {
        removed_count += 1;
        removed_1_to_2 = true;
        log_link_removed(&ticket1.id, &ticket2.id);
    }
    if ticket2.remove_from_array_field("links", &ticket1.id)? {
        removed_count += 1;
        removed_2_to_1 = true;
        log_link_removed(&ticket2.id, &ticket1.id);
    }

    if removed_count == 0 {
        return Err(JanusError::LinkNotFound);
    }

    // Warn about partial removal (only one direction succeeded)
    if removed_count == 1 {
        if removed_1_to_2 && !removed_2_to_1 {
            eprintln!(
                "Warning: Removed link from {} -> {} but not vice versa. Link state may be asymmetric.",
                ticket1.id, ticket2.id
            );
        } else if removed_2_to_1 && !removed_1_to_2 {
            eprintln!(
                "Warning: Removed link from {} -> {} but not vice versa. Link state may be asymmetric.",
                ticket2.id, ticket1.id
            );
        }
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
