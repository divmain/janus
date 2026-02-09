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
    "design",
    "acceptance",
    "description",
];

/// Validate a priority value
fn validate_priority(value: &str) -> Result<TicketPriority> {
    value.parse().map_err(|_| JanusError::InvalidFieldValue {
        field: "priority".to_string(),
        value: value.to_string(),
        valid_values: VALID_PRIORITIES.iter().map(|s| s.to_string()).collect(),
    })
}

/// Validate a ticket type value
fn validate_type(value: &str) -> Result<TicketType> {
    value.parse().map_err(|_| JanusError::InvalidFieldValue {
        field: "type".to_string(),
        value: value.to_string(),
        valid_values: VALID_TYPES.iter().map(|s| s.to_string()).collect(),
    })
}

/// Validate a status value
fn validate_status(value: &str) -> Result<TicketStatus> {
    TicketStatus::from_str(value).map_err(|_| JanusError::InvalidFieldValue {
        field: "status".to_string(),
        value: value.to_string(),
        valid_values: VALID_STATUSES.iter().map(|s| s.to_string()).collect(),
    })
}

/// Validate a size value
fn validate_size(value: &str) -> Result<TicketSize> {
    value.parse().map_err(|_| JanusError::InvalidFieldValue {
        field: "size".to_string(),
        value: value.to_string(),
        valid_values: VALID_SIZES.iter().map(|s| s.to_string()).collect(),
    })
}

/// Validate a parent ticket exists and is not self-referencing
async fn validate_parent(value: &str, ticket: &Ticket) -> Result<String> {
    let parent_ticket = Ticket::find(value).await?;
    if parent_ticket.id == ticket.id {
        return Err(JanusError::SelfParentTicket);
    }
    Ok(parent_ticket.id)
}

/// Format a field change for display
fn format_field_change(prev: Option<&str>, new: &str) -> (String, String) {
    let prev_display = prev.unwrap_or("(none)").to_string();
    let new_display = if new.is_empty() {
        "(none)".to_string()
    } else {
        new.to_string()
    };
    (prev_display, new_display)
}

/// Set a field on a ticket
pub async fn cmd_set(id: &str, field: &str, value: Option<&str>, output_json: bool) -> Result<()> {
    let ticket = Ticket::find(id).await?;
    let metadata = ticket.read()?;

    // Validate field name
    if !SUPPORTED_FIELDS.contains(&field) {
        return Err(JanusError::InvalidInput(format!(
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
            validate_priority(value)?;
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
            validate_type(value)?;
            new_value = value.to_string();
            ticket.update_field("type", value)?;
        }
        "parent" => {
            previous_value = metadata.parent.clone();
            if let Some(value) = value {
                let parent_id = validate_parent(value, &ticket).await?;
                new_value = parent_id.clone();
                ticket.update_field("parent", &parent_id)?;
            } else {
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
            validate_status(value)?;
            new_value = value.to_string();
            ticket.update_field("status", value)?;
        }
        "external_ref" => {
            previous_value = metadata.external_ref.clone();
            if let Some(value) = value {
                new_value = value.to_string();
                ticket.update_field("external_ref", value)?;
            } else {
                ticket.remove_field("external_ref")?;
                new_value = String::new();
            }
        }
        "size" => {
            previous_value = metadata.size.map(|s| s.to_string());
            if let Some(value) = value {
                validate_size(value)?;
                new_value = value.to_string();
                ticket.update_field("size", value)?;
            } else {
                ticket.remove_field("size")?;
                new_value = String::new();
            }
        }
        "design" => {
            previous_value = ticket.extract_section("Design")?;
            if let Some(value) = value {
                new_value = value.to_string();
                ticket.update_section("Design", Some(&new_value))?;
            } else {
                ticket.update_section("Design", None)?;
                new_value = String::new();
            }
        }
        "acceptance" => {
            previous_value = ticket.extract_section("Acceptance Criteria")?;
            if let Some(value) = value {
                new_value = value.to_string();
                ticket.update_section("Acceptance Criteria", Some(&new_value))?;
            } else {
                ticket.update_section("Acceptance Criteria", None)?;
                new_value = String::new();
            }
        }
        "description" => {
            previous_value = ticket.extract_description()?;
            if let Some(value) = value {
                new_value = value.to_string();
                ticket.update_description(Some(&new_value))?;
            } else {
                ticket.update_description(None)?;
                new_value = String::new();
            }
        }
        _ => unreachable!(), // Already validated above
    }

    let (prev_display, new_display) = format_field_change(previous_value.as_deref(), &new_value);

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
