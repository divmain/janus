use serde_json::json;

use super::CommandOutput;
use crate::cli::OutputOptions;
use crate::error::{JanusError, Result};
use crate::ticket::{ArrayField, Ticket};

/// Add symmetric links between tickets
pub async fn cmd_link_add(ids: &[String], output: OutputOptions) -> Result<()> {
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
                let has_existing_link = ticket.has_in_array_field(ArrayField::Links, &other.id)?;
                let other_has_link = other.has_in_array_field(ArrayField::Links, &ticket.id)?;

                // Detect one-way link: other -> ticket exists, but ticket -> other does not yet
                if !has_existing_link && other_has_link {
                    asymmetric_warnings.push((other.id.clone(), ticket.id.clone()));
                }

                if ticket.add_to_array_field(ArrayField::Links, &other.id)? {
                    added_count += 1;
                    // Event logging is now handled in Ticket::add_to_array_field at the domain layer
                }
            }
        }
    }

    // Report any one-way links that were detected and fixed
    let asymmetric_messages: Vec<String> = asymmetric_warnings
        .iter()
        .map(|(from, to)| {
            format!(
                "Detected one-way link from {from} to {to}; added reverse link to restore symmetry."
            )
        })
        .collect();

    if !output.json {
        for msg in &asymmetric_messages {
            eprintln!("{msg}");
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
        format!("Added {added_count} link(s) between {num_tickets} tickets")
    };

    let mut json_payload = json!({
        "action": if added_count > 0 { "linked" } else { "already_linked" },
        "tickets": ticket_ids,
        "links_added": added_count,
        "links_updated": links_updated,
    });

    if !asymmetric_messages.is_empty() {
        json_payload["warnings"] = json!(asymmetric_messages);
    }

    CommandOutput::new(json_payload)
        .with_text(text)
        .print(output)
}

/// Remove symmetric links between two tickets
pub async fn cmd_link_remove(id1: &str, id2: &str, output: OutputOptions) -> Result<()> {
    let ticket1 = Ticket::find(id1).await?;
    let ticket2 = Ticket::find(id2).await?;

    let mut removed_count = 0;
    let mut removed_1_to_2 = false;
    let mut removed_2_to_1 = false;

    if ticket1.remove_from_array_field(ArrayField::Links, &ticket2.id)? {
        removed_count += 1;
        removed_1_to_2 = true;
        // Event logging is now handled in Ticket::remove_from_array_field at the domain layer
    }
    if ticket2.remove_from_array_field(ArrayField::Links, &ticket1.id)? {
        removed_count += 1;
        removed_2_to_1 = true;
        // Event logging is now handled in Ticket::remove_from_array_field at the domain layer
    }

    if removed_count == 0 {
        return Err(JanusError::LinkNotFound);
    }

    // Report if only one direction existed (pre-existing asymmetry now cleaned up)
    let asymmetric_warning = if removed_count == 1 {
        let (from, to) = if removed_1_to_2 && !removed_2_to_1 {
            (&ticket1.id, &ticket2.id)
        } else {
            (&ticket2.id, &ticket1.id)
        };
        let msg = format!(
            "Detected one-way link from {from} to {to} (reverse link did not exist). Removed the one-way link."
        );
        if !output.json {
            eprintln!("{msg}");
        }
        Some(msg)
    } else {
        None
    };

    let metadata1 = ticket1.read()?;
    let metadata2 = ticket2.read()?;
    let mut links_updated = serde_json::Map::new();
    links_updated.insert(ticket1.id.clone(), json!(metadata1.links));
    links_updated.insert(ticket2.id.clone(), json!(metadata2.links));

    let mut json_payload = json!({
        "action": "unlinked",
        "tickets": [&ticket1.id, &ticket2.id],
        "links_updated": links_updated,
    });

    if let Some(ref warning) = asymmetric_warning {
        json_payload["warnings"] = json!([warning]);
    }

    CommandOutput::new(json_payload)
        .with_text(format!("Removed link: {} <-> {}", ticket1.id, ticket2.id))
        .print(output)
}
