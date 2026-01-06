use crate::error::{JanusError, Result};
use crate::ticket::Ticket;

/// Add symmetric links between tickets
pub fn cmd_link_add(ids: &[String]) -> Result<()> {
    if ids.len() < 2 {
        return Err(JanusError::Other(
            "At least two ticket IDs are required".to_string(),
        ));
    }

    // Find all tickets first to validate they exist
    let tickets: Vec<Ticket> = ids
        .iter()
        .map(|id| Ticket::find(id))
        .collect::<Result<Vec<_>>>()?;

    let mut added_count = 0;

    // Add links between all pairs
    for ticket in &tickets {
        for other in &tickets {
            if ticket.id != other.id && ticket.add_to_array_field("links", &other.id)? {
                added_count += 1;
            }
        }
    }

    if added_count == 0 {
        println!("All links already exist");
    } else {
        println!(
            "Added {} link(s) between {} tickets",
            added_count,
            tickets.len()
        );
    }

    Ok(())
}

/// Remove symmetric links between two tickets
pub fn cmd_link_remove(id1: &str, id2: &str) -> Result<()> {
    let ticket1 = Ticket::find(id1)?;
    let ticket2 = Ticket::find(id2)?;

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

    println!("Removed link: {} <-> {}", ticket1.id, ticket2.id);

    Ok(())
}
