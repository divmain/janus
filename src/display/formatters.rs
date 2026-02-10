//! Output formatters for displaying tickets, plans, and other entities

use std::collections::HashMap;

use crate::commands::NextItemResult;
use crate::types::TicketMetadata;
use owo_colors::OwoColorize;

/// Ticket display formatters
pub struct TicketFormatter;

impl TicketFormatter {
    /// Format a section of related tickets (blockers, blocking, children)
    pub fn format_section(title: &str, items: &[&TicketMetadata]) -> String {
        if items.is_empty() {
            return String::new();
        }
        let mut output = format!("\n\n## {title}");
        for item in items {
            output.push_str(&format!("\n{}", crate::display::format_ticket_bullet(item)));
        }
        output
    }
}

/// Plan next command formatter
pub struct PlanNextFormatter;

impl PlanNextFormatter {
    /// Format a next item with its tickets as a string
    pub fn format_next_item(
        item: &NextItemResult,
        ticket_map: &HashMap<String, TicketMetadata>,
    ) -> String {
        let mut output = String::new();
        output.push_str(&format!(
            "{}",
            format!("## Next: Phase {} - {}", item.phase_number, item.phase_name).bold()
        ));
        output.push('\n');

        for (i, (ticket_id, ticket_meta)) in item.tickets.iter().enumerate() {
            output.push_str(&Self::format_ticket(ticket_id, ticket_meta, ticket_map));

            if i < item.tickets.len() - 1 {
                output.push('\n');
            }
        }
        output.push('\n');
        output
    }

    /// Print a next item with its tickets
    pub fn print_next_item(item: &NextItemResult, ticket_map: &HashMap<String, TicketMetadata>) {
        print!("{}", Self::format_next_item(item, ticket_map));
    }

    /// Format a single ticket within a next item as a string
    fn format_ticket(
        ticket_id: &str,
        ticket_meta: &Option<TicketMetadata>,
        ticket_map: &HashMap<String, TicketMetadata>,
    ) -> String {
        let mut output = String::new();
        let status = ticket_meta
            .as_ref()
            .and_then(|t| t.status)
            .unwrap_or_default();
        let status_badge = crate::display::format_status_colored(status);
        let title = ticket_meta
            .as_ref()
            .and_then(|t| t.title.as_deref())
            .unwrap_or("");

        output.push_str(&format!(
            "{} {} {}\n",
            status_badge,
            ticket_id.cyan(),
            title
        ));

        if let Some(meta) = ticket_meta {
            output.push_str(&Self::format_priority(meta));
            output.push_str(&Self::format_deps(meta, ticket_map));
        }
        output
    }

    /// Format ticket priority as a string
    fn format_priority(meta: &TicketMetadata) -> String {
        let priority = meta.priority.map(|p| p.as_num()).unwrap_or(2);
        format!("  Priority: P{priority}\n")
    }

    /// Format ticket dependencies with their status as a string
    fn format_deps(meta: &TicketMetadata, ticket_map: &HashMap<String, TicketMetadata>) -> String {
        if !meta.deps.is_empty() {
            let deps_with_status: Vec<String> = meta
                .deps
                .iter()
                .map(|dep| {
                    let dep_status = ticket_map
                        .get(dep)
                        .and_then(|t| t.status)
                        .map(|s| format!("[{s}]"))
                        .unwrap_or_else(|| "[missing]".to_string());
                    format!("{dep} {dep_status}")
                })
                .collect();
            format!("  Deps: {}\n", deps_with_status.join(", "))
        } else {
            String::new()
        }
    }
}
