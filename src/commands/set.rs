use serde_json::json;

use crate::error::{JanusError, Result};
use crate::ticket::Ticket;
use crate::types::{TicketPriority, TicketType, VALID_PRIORITIES, VALID_TYPES};

/// Supported fields for the set command
const SUPPORTED_FIELDS: &[&str] = &["priority", "type", "parent"];

/// Set a field on a ticket
pub fn cmd_set(id: &str, field: &str, value: &str, output_json: bool) -> Result<()> {
    let ticket = Ticket::find(id)?;
    let metadata = ticket.read()?;

    // Validate field name
    if !SUPPORTED_FIELDS.contains(&field) {
        return Err(JanusError::InvalidField {
            field: field.to_string(),
            valid_fields: SUPPORTED_FIELDS.iter().map(|s| s.to_string()).collect(),
        });
    }

    // Get previous value and validate/update based on field type
    let previous_value: Option<String>;
    let new_value: String;

    match field {
        "priority" => {
            previous_value = metadata.priority.map(|p| p.to_string());
            // Validate priority
            let _parsed: TicketPriority =
                value.parse().map_err(|_| JanusError::InvalidFieldValue {
                    field: field.to_string(),
                    value: value.to_string(),
                    valid_values: VALID_PRIORITIES.iter().map(|s| s.to_string()).collect(),
                })?;
            new_value = value.to_string();
            ticket.update_field("priority", value)?;
        }
        "type" => {
            previous_value = metadata.ticket_type.map(|t| t.to_string());
            // Validate type
            let _parsed: TicketType = value.parse().map_err(|_| JanusError::InvalidFieldValue {
                field: field.to_string(),
                value: value.to_string(),
                valid_values: VALID_TYPES.iter().map(|s| s.to_string()).collect(),
            })?;
            new_value = value.to_string();
            ticket.update_field("type", value)?;
        }
        "parent" => {
            previous_value = metadata.parent.clone();
            if value.is_empty() {
                // Clear parent
                ticket.remove_field("parent")?;
                new_value = String::new();
            } else {
                // Validate parent ticket exists
                let parent_ticket = Ticket::find(value)?;
                new_value = parent_ticket.id.clone();
                ticket.update_field("parent", &parent_ticket.id)?;
            }
        }
        _ => unreachable!(), // Already validated above
    }

    if output_json {
        let output = json!({
            "id": ticket.id,
            "action": "field_updated",
            "field": field,
            "previous_value": previous_value,
            "new_value": new_value,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        let prev_display = previous_value.as_deref().unwrap_or("(none)");
        let new_display = if new_value.is_empty() {
            "(none)"
        } else {
            &new_value
        };
        println!(
            "Updated {} field '{}': {} -> {}",
            ticket.id, field, prev_display, new_display
        );
    }

    Ok(())
}
