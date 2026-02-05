use serde_json::json;

use super::CommandOutput;
use crate::error::{JanusError, Result};
use crate::events::log_field_updated;
use crate::parser::parse_document;
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

/// Validate external_ref (free-form string)
fn validate_external_ref(value: &str) -> String {
    value.to_string()
}

/// Validate design (free-form text, no validation needed)
fn validate_design(value: &str) -> String {
    value.to_string()
}

/// Validate acceptance (free-form text, no validation needed)
fn validate_acceptance(value: &str) -> String {
    value.to_string()
}

/// Validate description (free-form text, no validation needed)
fn validate_description(value: &str) -> String {
    value.to_string()
}

/// Extract current value of a body section from ticket content
fn extract_section_content(ticket: &Ticket, section_name: &str) -> Result<Option<String>> {
    let content = ticket.read_content()?;
    let doc = parse_document(&content).map_err(|e| {
        JanusError::InvalidFormat(format!("Failed to parse ticket {}: {}", ticket.id, e))
    })?;
    Ok(doc.extract_section(section_name))
}

/// Extract the description (content between title and first H2)
fn extract_description(ticket: &Ticket) -> Result<Option<String>> {
    let content = ticket.read_content()?;
    let doc = parse_document(&content).map_err(|e| {
        JanusError::InvalidFormat(format!("Failed to parse ticket {}: {}", ticket.id, e))
    })?;

    // Get body without title
    let body = &doc.body;
    let title_end = body.find('\n').unwrap_or(0);
    let after_title = &body[title_end..].trim_start();

    // Find first H2 or end of document
    if let Some(h2_pos) = after_title.find("\n## ") {
        let desc = after_title[..h2_pos].trim();
        if desc.is_empty() {
            Ok(None)
        } else {
            Ok(Some(desc.to_string()))
        }
    } else {
        // No H2 sections, everything after title is description
        let desc = after_title.trim();
        if desc.is_empty() {
            Ok(None)
        } else {
            Ok(Some(desc.to_string()))
        }
    }
}

/// Update a body section in a ticket
fn update_body_section(ticket: &Ticket, section_name: &str, content: Option<&str>) -> Result<()> {
    let raw_content = ticket.read_content()?;
    let doc = parse_document(&raw_content).map_err(|e| {
        JanusError::InvalidFormat(format!(
            "Failed to parse ticket {} at {}: {}",
            ticket.id,
            crate::utils::format_relative_path(&ticket.file_path),
            e
        ))
    })?;

    let updated_body = if let Some(new_content) = content {
        doc.update_section(section_name, new_content)
    } else {
        // Remove the section if content is None
        let pattern = format!(r"(?ims)^##\s+{}\s*\n.*?", regex::escape(section_name));
        let section_re = regex::Regex::new(&pattern).expect("section regex should be valid");
        section_re.replace(&doc.body, "").to_string()
    };

    let new_content = format!("---\n{}\n---\n{}", doc.frontmatter_raw, updated_body);
    ticket.write(&new_content)
}

/// Update the description (content between title and first H2)
fn update_description(ticket: &Ticket, description: Option<&str>) -> Result<()> {
    let raw_content = ticket.read_content()?;
    let doc = parse_document(&raw_content).map_err(|e| {
        JanusError::InvalidFormat(format!(
            "Failed to parse ticket {} at {}: {}",
            ticket.id,
            crate::utils::format_relative_path(&ticket.file_path),
            e
        ))
    })?;

    // Get body without title
    let body = &doc.body;
    let title_end = body.find('\n').unwrap_or(body.len());
    let title = &body[..title_end];
    let after_title = &body[title_end..];

    // Find first H2 or end of document
    let h2_pos = after_title.find("\n## ");

    let new_body = if let Some(pos) = h2_pos {
        let from_h2 = &after_title[pos..];
        if let Some(desc) = description {
            format!("{title}\n\n{desc}{from_h2}")
        } else {
            format!("{title}{from_h2}")
        }
    } else {
        // No H2 sections
        if let Some(desc) = description {
            format!("{title}\n\n{desc}")
        } else {
            title.to_string()
        }
    };

    let new_content = format!("---\n{}\n---\n{}", doc.frontmatter_raw, new_body);
    ticket.write(&new_content)
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
                new_value = validate_external_ref(value);
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
            previous_value = extract_section_content(&ticket, "Design")?;
            if let Some(value) = value {
                new_value = validate_design(value);
                update_body_section(&ticket, "Design", Some(&new_value))?;
            } else {
                update_body_section(&ticket, "Design", None)?;
                new_value = String::new();
            }
        }
        "acceptance" => {
            previous_value = extract_section_content(&ticket, "Acceptance Criteria")?;
            if let Some(value) = value {
                new_value = validate_acceptance(value);
                update_body_section(&ticket, "Acceptance Criteria", Some(&new_value))?;
            } else {
                update_body_section(&ticket, "Acceptance Criteria", None)?;
                new_value = String::new();
            }
        }
        "description" => {
            previous_value = extract_description(&ticket)?;
            if let Some(value) = value {
                new_value = validate_description(value);
                update_description(&ticket, Some(&new_value))?;
            } else {
                update_description(&ticket, None)?;
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
