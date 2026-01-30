use serde_json::json;

use super::CommandOutput;
use crate::error::{JanusError, Result};
use crate::events::log_field_updated;
use crate::ticket::Ticket;
use std::str::FromStr;

use crate::types::{
    TicketPriority, TicketSize, TicketStatus, TicketType, VALID_PRIORITIES, VALID_SIZES,
    VALID_STATUSES, VALID_TYPES,
};

/// Supported fields for the set command
const SUPPORTED_FIELDS: &[&str] = &[
    "priority",
    "type",
    "parent",
    "status",
    "external_ref",
    "size",
];

/// Set a field on a ticket
pub async fn cmd_set(id: &str, field: &str, value: Option<&str>, output_json: bool) -> Result<()> {
    let ticket = Ticket::find(id).await?;
    let metadata = ticket.read()?;

    // Validate field name
    if !SUPPORTED_FIELDS.contains(&field) {
        return Err(JanusError::Other(format!(
            "invalid field '{}'. Must be one of: {}",
            field,
            SUPPORTED_FIELDS.join(", ")
        )));
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
        "status" => {
            previous_value = metadata.status.map(|s| s.to_string());
            let value = value.ok_or_else(|| JanusError::InvalidFieldValue {
                field: field.to_string(),
                value: "(none)".to_string(),
                valid_values: VALID_STATUSES.iter().map(|s| s.to_string()).collect(),
            })?;
            // Validate status
            let _parsed: TicketStatus =
                TicketStatus::from_str(value).map_err(|_| JanusError::InvalidFieldValue {
                    field: field.to_string(),
                    value: value.to_string(),
                    valid_values: VALID_STATUSES.iter().map(|s| s.to_string()).collect(),
                })?;
            new_value = value.to_string();
            ticket.update_field("status", value)?;
        }
        "external_ref" => {
            previous_value = metadata.external_ref.clone();
            if let Some(value) = value {
                new_value = value.to_string();
                ticket.update_field("external_ref", value)?;
            } else {
                // Clear external_ref
                ticket.remove_field("external_ref")?;
                new_value = String::new();
            }
        }
        "size" => {
            previous_value = metadata.size.map(|s| s.to_string());
            if let Some(value) = value {
                // Validate size
                let _parsed: TicketSize =
                    value.parse().map_err(|_| JanusError::InvalidFieldValue {
                        field: field.to_string(),
                        value: value.to_string(),
                        valid_values: VALID_SIZES.iter().map(|s| s.to_string()).collect(),
                    })?;
                new_value = value.to_string();
                ticket.update_field("size", value)?;
            } else {
                // Clear size
                ticket.remove_field("size")?;
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
