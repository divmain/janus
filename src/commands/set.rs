use serde_json::json;

use super::CommandOutput;
use crate::error::{JanusError, Result};
use crate::events::log_field_updated;
use crate::ticket::Ticket;
use crate::types::{TicketPriority, TicketType, VALID_PRIORITIES, VALID_TYPES};

/// Supported fields for the set command
const SUPPORTED_FIELDS: &[&str] = &["priority", "type", "parent"];

/// Set a field on a ticket
pub async fn cmd_set(id: &str, field: &str, value: Option<&str>, output_json: bool) -> Result<()> {
    let ticket = Ticket::find(id).await?;
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
            let value = value.ok_or_else(|| JanusError::InvalidFieldValue {
                field: field.to_string(),
                value: "(none)".to_string(),
                valid_values: VALID_PRIORITIES.iter().map(|s| s.to_string()).collect(),
            })?;
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
            let value = value.ok_or_else(|| JanusError::InvalidFieldValue {
                field: field.to_string(),
                value: "(none)".to_string(),
                valid_values: VALID_TYPES.iter().map(|s| s.to_string()).collect(),
            })?;
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
            if let Some(value) = value {
                // Validate parent ticket exists
                let parent_ticket = Ticket::find(value).await?;
                if parent_ticket.id == ticket.id {
                    return Err(JanusError::SelfParentTicket);
                }
                new_value = parent_ticket.id.clone();
                ticket.update_field("parent", &parent_ticket.id)?;
            } else {
                // Clear parent
                ticket.remove_field("parent")?;
                new_value = String::new();
            }
        }
        _ => unreachable!(), // Already validated above
    }

    let prev_display = previous_value.as_deref().unwrap_or("(none)").to_string();
    let new_display = if new_value.is_empty() {
        "(none)".to_string()
    } else {
        new_value.clone()
    };

    // Log the event
    log_field_updated(&ticket.id, field, previous_value.as_deref(), &new_value);

    CommandOutput::new(json!({
        "id": ticket.id,
        "action": "field_updated",
        "field": field,
        "previous_value": previous_value,
        "new_value": new_value,
    }))
    .with_text(format!(
        "Updated {} field '{}': {} -> {}",
        ticket.id, field, prev_display, new_display
    ))
    .print(output_json)
}
