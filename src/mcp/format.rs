//! Ticket and plan formatting utilities for MCP output.
//!
//! This module provides centralized helper functions for formatting tickets
//! and plans as markdown for LLM consumption. It eliminates duplicated
//! formatting logic across the MCP tools.

use crate::types::{TicketMetadata, TicketStatus};
use std::collections::HashMap;

// ============================================================================
// Ticket Field Formatting Helpers
// ============================================================================

/// Format a ticket ID with a fallback for missing values.
pub fn format_ticket_id(metadata: &TicketMetadata) -> &str {
    metadata.id.as_deref().unwrap_or("unknown")
}

/// Format a ticket title with a fallback for missing values.
pub fn format_ticket_title(metadata: &TicketMetadata) -> &str {
    metadata.title.as_deref().unwrap_or("Untitled")
}

/// Format a ticket status as a string, with "new" as default.
pub fn format_ticket_status(metadata: &TicketMetadata) -> String {
    metadata
        .status
        .map(|s| s.to_string())
        .unwrap_or_else(|| "new".to_string())
}

/// Format a ticket type as a string, with "task" as default.
pub fn format_ticket_type(metadata: &TicketMetadata) -> String {
    metadata
        .ticket_type
        .map(|t| t.to_string())
        .unwrap_or_else(|| "task".to_string())
}

/// Format a ticket priority as a badge string (e.g., "P2").
pub fn format_ticket_priority(metadata: &TicketMetadata) -> String {
    metadata
        .priority
        .map(|p| format!("P{}", p.as_num()))
        .unwrap_or_else(|| "P2".to_string())
}

/// Format a ticket size as a string, with "-" as default for missing values.
pub fn format_ticket_size(metadata: &TicketMetadata) -> String {
    metadata
        .size
        .map(|s| s.to_string())
        .unwrap_or_else(|| "-".to_string())
}

/// Format a ticket depth as a string, with "0" as default for root tickets.
pub fn format_ticket_depth(metadata: &TicketMetadata) -> String {
    metadata
        .depth
        .map(|d| d.to_string())
        .unwrap_or_else(|| "0".to_string())
}

// ============================================================================
// Ticket Relationship Formatting
// ============================================================================

/// Format a related ticket as a list item with status badge.
/// Used for blockers, blocking, and children sections.
pub fn format_related_ticket_line(metadata: &TicketMetadata) -> String {
    let id = format_ticket_id(metadata);
    let title = format_ticket_title(metadata);
    let status = format_ticket_status(metadata);
    format!("- **{id}**: {title} [{status}]\n")
}

/// Format a single ticket line for plan status display.
/// Returns (checkbox_char, title, status_suffix_with_newline).
pub fn format_plan_ticket_line(
    ticket_id: &str,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> (char, String, String) {
    if let Some(ticket) = ticket_map.get(ticket_id) {
        let status = ticket.status.unwrap_or(TicketStatus::New);
        let checkbox = if status == TicketStatus::Complete {
            'x'
        } else {
            ' '
        };
        let title = format_ticket_title(ticket).to_string();
        let status_suffix = if status == TicketStatus::InProgress {
            " (in_progress)\n".to_string()
        } else {
            "\n".to_string()
        };
        (checkbox, title, status_suffix)
    } else {
        // Ticket not found
        (' ', "Unknown ticket".to_string(), "\n".to_string())
    }
}

// ============================================================================
// Table Formatting
// ============================================================================

/// Format a ticket as a markdown table row with standard columns.
/// Columns: ID, Title, Status, Type, Priority, Size
pub fn format_ticket_table_row(metadata: &TicketMetadata) -> String {
    let id = format_ticket_id(metadata);
    let title = format_ticket_title(metadata);
    let status = format_ticket_status(metadata);
    let ticket_type = format_ticket_type(metadata);
    let priority = format_ticket_priority(metadata);
    let size = format_ticket_size(metadata);

    format!("| {id} | {title} | {status} | {ticket_type} | {priority} | {size} |\n")
}

/// Format a ticket as a markdown table row with children-specific columns.
/// Columns: ID, Title, Status, Depth
pub fn format_children_table_row(metadata: &TicketMetadata) -> String {
    let id = format_ticket_id(metadata);
    let title = format_ticket_title(metadata);
    let status = format_ticket_status(metadata);
    let depth = format_ticket_depth(metadata);

    format!("| {id} | {title} | {status} | {depth} |\n")
}

/// Format a metadata field row for the ticket details table.
pub fn format_metadata_field_row(name: &str, value: Option<&str>) -> Option<String> {
    value.map(|v| format!("| {name} | {v} |\n"))
}

// ============================================================================
// Section Formatting
// ============================================================================

/// Format a section of related tickets (blockers, blocking, children).
pub fn format_related_tickets_section(
    section_title: &str,
    tickets: &[&TicketMetadata],
) -> Option<String> {
    if tickets.is_empty() {
        return None;
    }

    let mut output = format!("\n## {section_title}\n\n");
    for ticket in tickets {
        output.push_str(&format_related_ticket_line(ticket));
    }
    Some(output)
}

// ============================================================================
// Plan Formatting
// ============================================================================

/// Format a plan ticket entry for plan status display.
pub fn format_plan_ticket_entry(
    ticket_id: &str,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> String {
    let (checkbox, title, status_suffix) = format_plan_ticket_line(ticket_id, ticket_map);
    format!("- [{checkbox}] {ticket_id}: {title}{status_suffix}")
}

/// Format a spawn context entry for a child ticket.
pub fn format_spawn_context_line(metadata: &TicketMetadata) -> Option<String> {
    let id = format_ticket_id(metadata);
    metadata
        .spawn_context
        .as_ref()
        .map(|ctx| format!("- **{id}**: \"{ctx}\"\n"))
}
