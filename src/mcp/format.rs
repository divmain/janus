//! Ticket and plan formatting utilities for MCP output.
//!
//! This module provides centralized helper functions for formatting tickets
//! and plans as markdown for LLM consumption. It eliminates duplicated
//! formatting logic across the MCP tools.

use crate::next::{InclusionReason, WorkItem};
use crate::plan::compute_all_phase_statuses;
use crate::plan::types::{PlanMetadata, PlanStatus};
use crate::types::{TicketData, TicketMetadata, TicketStatus};
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

// ============================================================================
// Complex Markdown Formatters (moved from tools.rs)
// ============================================================================

/// Format a ticket as markdown for LLM consumption
pub fn format_ticket_as_markdown(
    metadata: &TicketMetadata,
    content: &str,
    blockers: &[&TicketMetadata],
    blocking: &[&TicketMetadata],
    children: &[&TicketMetadata],
) -> String {
    let mut output = String::new();

    // Title with ID
    let id = format_ticket_id(metadata);
    let title = format_ticket_title(metadata);
    output.push_str(&format!("# {id}: {title}\n\n"));

    // Metadata table
    output.push_str("| Field | Value |\n");
    output.push_str("|-------|-------|\n");

    if let Some(status) = metadata.status {
        output.push_str(&format!("| Status | {status} |\n"));
    }
    if let Some(ticket_type) = metadata.ticket_type {
        output.push_str(&format!("| Type | {ticket_type} |\n"));
    }
    if let Some(priority) = metadata.priority {
        output.push_str(&format!("| Priority | P{} |\n", priority.as_num()));
    }
    if let Some(size) = metadata.size {
        output.push_str(&format!("| Size | {size} |\n"));
    }
    if let Some(ref created) = metadata.created {
        // Extract just the date portion (YYYY-MM-DD) from the ISO timestamp
        let date = created.split('T').next().unwrap_or(created);
        output.push_str(&format!("| Created | {date} |\n"));
    }
    if !metadata.deps.is_empty() {
        output.push_str(&format!(
            "| Dependencies | {} |\n",
            metadata.deps.join(", ")
        ));
    }
    if !metadata.links.is_empty() {
        output.push_str(&format!("| Links | {} |\n", metadata.links.join(", ")));
    }
    if let Some(ref parent) = metadata.parent {
        output.push_str(&format!("| Parent | {parent} |\n"));
    }
    if let Some(ref spawned_from) = metadata.spawned_from {
        output.push_str(&format!("| Spawned From | {spawned_from} |\n"));
    }
    if let Some(ref spawn_context) = metadata.spawn_context {
        output.push_str(&format!("| Spawn Context | {spawn_context} |\n"));
    }
    if let Some(depth) = metadata.depth {
        output.push_str(&format!("| Depth | {depth} |\n"));
    }
    if let Some(ref external_ref) = metadata.external_ref {
        output.push_str(&format!("| External Ref | {external_ref} |\n"));
    }
    if let Some(ref remote) = metadata.remote {
        output.push_str(&format!("| Remote | {remote} |\n"));
    }

    // Description section (the ticket body content)
    output.push_str("\n## Description\n\n");
    output.push_str(content.trim());
    output.push('\n');

    // Completion summary section (if present)
    if let Some(ref summary) = metadata.completion_summary {
        output.push_str("\n## Completion Summary\n\n");
        output.push_str(summary.trim());
        output.push('\n');
    }

    // Blockers section
    if let Some(section) = format_related_tickets_section("Blockers", blockers) {
        output.push_str(&section);
    }

    // Blocking section
    if let Some(section) = format_related_tickets_section("Blocking", blocking) {
        output.push_str(&section);
    }

    // Children section
    if let Some(section) = format_related_tickets_section("Children", children) {
        output.push_str(&section);
    }

    output
}

/// Build a human-readable filter summary from filter parameters
pub fn build_filter_summary(
    ready: Option<bool>,
    blocked: Option<bool>,
    status: Option<&str>,
    ticket_type: Option<&str>,
    spawned_from: Option<&str>,
    depth: Option<u32>,
    size: Option<&str>,
) -> String {
    let mut filters = Vec::new();

    if ready == Some(true) {
        filters.push("ready tickets".to_string());
    }
    if blocked == Some(true) {
        filters.push("blocked tickets".to_string());
    }
    if let Some(s) = status {
        filters.push(format!("status={s}"));
    }
    if let Some(t) = ticket_type {
        filters.push(format!("type={t}"));
    }
    if let Some(sf) = spawned_from {
        filters.push(format!("spawned_from={sf}"));
    }
    if let Some(d) = depth {
        filters.push(format!("depth={d}"));
    }
    if let Some(sz) = size {
        filters.push(format!("size={sz}"));
    }

    if filters.is_empty() {
        String::new()
    } else {
        format!("**Showing:** {}\n\n", filters.join(", "))
    }
}

/// Format a list of tickets as markdown for LLM consumption
pub fn format_ticket_list_as_markdown(tickets: &[&TicketMetadata], filter_summary: &str) -> String {
    let mut output = String::new();

    // Header
    output.push_str("# Tickets\n\n");

    // Filter summary if any filters were applied
    if !filter_summary.is_empty() {
        output.push_str(filter_summary);
    }

    // Handle empty results
    if tickets.is_empty() {
        output.push_str("No tickets found matching criteria.\n");
        return output;
    }

    // Table header
    output.push_str("| ID | Title | Status | Type | Priority | Size |\n");
    output.push_str("|----|-------|--------|------|----------|------|\n");

    // Table rows using centralized formatting
    for ticket in tickets {
        output.push_str(&format_ticket_table_row(ticket));
    }

    // Total count
    output.push_str(&format!("\n**Total:** {} tickets\n", tickets.len()));

    output
}

/// Format plan status as markdown for LLM consumption
pub fn format_plan_status_as_markdown(
    plan_id: &str,
    metadata: &PlanMetadata,
    plan_status: &PlanStatus,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> String {
    let mut output = String::new();

    // Title and ID
    let title = metadata.title.as_deref().unwrap_or("Untitled");
    output.push_str(&format!("# Plan: {plan_id} - {title}\n\n"));

    // Overall status and progress
    output.push_str(&format!("**Status:** {}  \n", plan_status.status));
    output.push_str(&format!(
        "**Progress:** {}/{} tickets complete ({}%)\n",
        plan_status.completed_count,
        plan_status.total_count,
        plan_status.progress_percent() as u32
    ));

    if metadata.is_phased() {
        // Phased plan: show phases with tickets
        let phase_statuses = compute_all_phase_statuses(metadata, ticket_map);

        for (phase, phase_status) in metadata.phases().iter().zip(phase_statuses.iter()) {
            output.push_str(&format!(
                "\n## Phase {}: {} ({})",
                phase.number, phase.name, phase_status.status
            ));

            for ticket_id in &phase.ticket_list.tickets {
                output.push_str(&format_plan_ticket_entry(ticket_id, ticket_map));
            }
        }
    } else {
        // Simple plan: show tickets in a single list
        let tickets = metadata.all_tickets();
        if !tickets.is_empty() {
            output.push_str("\n## Tickets\n");
            for ticket_id in tickets {
                output.push_str(&format_plan_ticket_entry(ticket_id, ticket_map));
            }
        }
    }

    output
}

/// Format children of a ticket as markdown for LLM consumption
pub fn format_children_as_markdown(
    parent_id: &str,
    parent_title: &str,
    children: &[&TicketMetadata],
) -> String {
    let mut output = String::new();

    // Header with parent info
    output.push_str(&format!("# Children of {parent_id}: {parent_title}\n\n"));

    // Handle empty results
    if children.is_empty() {
        output.push_str("No children found for this ticket.\n");
        return output;
    }

    // Spawned count
    output.push_str(&format!("**Spawned tickets:** {}\n\n", children.len()));

    // Table header
    output.push_str("| ID | Title | Status | Depth |\n");
    output.push_str("|----|-------|--------|-------|\n");

    // Table rows using centralized formatting
    for child in children {
        output.push_str(&format_children_table_row(child));
    }

    // Spawn contexts section (only if any children have spawn_context)
    let children_with_context: Vec<_> = children
        .iter()
        .filter(|c| c.spawn_context.is_some())
        .collect();

    if !children_with_context.is_empty() {
        output.push_str("\n**Spawn contexts:**\n");
        for child in children_with_context {
            if let Some(line) = format_spawn_context_line(child) {
                output.push_str(&line);
            }
        }
    }

    output
}

/// Format next work items as markdown for LLM consumption
pub fn format_next_work_as_markdown(
    work_items: &[WorkItem],
    ticket_map: &HashMap<String, TicketMetadata>,
) -> String {
    let mut output = String::new();

    // Header
    output.push_str("## Next Work Items\n\n");

    // Numbered list of work items
    for (idx, item) in work_items.iter().enumerate() {
        let ticket_id = &item.ticket_id;
        let priority = item.metadata.priority_num();
        let title = format_ticket_title(&item.metadata);
        let priority_badge = format!("[P{priority}]");

        // Format the main line with context
        let context = match &item.reason {
            InclusionReason::Blocking(target_id) => {
                format!(" *(blocks {target_id})*")
            }
            InclusionReason::TargetBlocked => " *(currently blocked)*".to_string(),
            InclusionReason::Ready => String::new(),
        };

        output.push_str(&format!(
            "{}. **{}** {} {}{}\n",
            idx + 1,
            ticket_id,
            priority_badge,
            title,
            context
        ));

        // Status line
        let status = match &item.reason {
            InclusionReason::Ready | InclusionReason::Blocking(_) => "ready",
            InclusionReason::TargetBlocked => "blocked",
        };
        output.push_str(&format!("   - Status: {status}\n"));

        // Additional context for blocked tickets
        if matches!(item.reason, InclusionReason::TargetBlocked) {
            let incomplete_deps: Vec<&String> = item
                .metadata
                .deps
                .iter()
                .filter(|dep_id| {
                    ticket_map
                        .get(*dep_id)
                        .map(|dep| dep.status != Some(TicketStatus::Complete))
                        .unwrap_or(false)
                })
                .collect();

            if !incomplete_deps.is_empty() {
                let dep_list: Vec<&str> = incomplete_deps.iter().map(|s| s.as_str()).collect();
                output.push_str(&format!("   - Waiting on: {}\n", dep_list.join(", ")));
            }
        }

        // Context about what this ticket blocks
        if let Some(blocks) = &item.blocks {
            output.push_str(&format!(
                "   - This ticket must be completed before {blocks} can be worked on\n"
            ));
        }

        output.push('\n');
    }

    // Find the first ready ticket for recommended action
    let first_ready = work_items.iter().find(|item| {
        matches!(
            item.reason,
            InclusionReason::Ready | InclusionReason::Blocking(_)
        )
    });

    if let Some(ready_item) = first_ready {
        let ready_title = format_ticket_title(&ready_item.metadata);
        output.push_str("### Recommended Action\n\n");
        output.push_str(&format!(
            "Start with **{}**: {}\n",
            ready_item.ticket_id, ready_title
        ));
    } else {
        // All items are blocked
        output.push_str("### Note\n\n");
        output.push_str("All listed tickets are currently blocked by dependencies. Consider working on the dependencies first or reviewing the dependency chain.\n");
    }

    output
}
