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
        let mut output = format!("\n\n## {}", title);
        for item in items {
            output.push_str(&format!("\n{}", crate::display::format_ticket_bullet(item)));
        }
        output
    }
}

/// Plan next command formatter
pub struct PlanNextFormatter;

impl PlanNextFormatter {
    /// Print a next item with its tickets
    pub fn print_next_item(item: &NextItemResult, ticket_map: &HashMap<String, TicketMetadata>) {
        println!(
            "{}",
            format!("## Next: Phase {} - {}", item.phase_number, item.phase_name).bold()
        );
        println!();

        for (i, (ticket_id, ticket_meta)) in item.tickets.iter().enumerate() {
            Self::print_ticket(ticket_id, ticket_meta, ticket_map);

            if i < item.tickets.len() - 1 {
                println!();
            }
        }
        println!();
    }

    /// Print a single ticket within a next item
    fn print_ticket(
        ticket_id: &str,
        ticket_meta: &Option<TicketMetadata>,
        ticket_map: &HashMap<String, TicketMetadata>,
    ) {
        let status = ticket_meta
            .as_ref()
            .and_then(|t| t.status)
            .unwrap_or_default();
        let status_badge = crate::display::format_status_colored(status);
        let title = ticket_meta
            .as_ref()
            .and_then(|t| t.title.as_deref())
            .unwrap_or("");

        println!("{} {} {}", status_badge, ticket_id.cyan(), title);

        if let Some(meta) = ticket_meta {
            Self::print_priority(meta);
            Self::print_deps(meta, ticket_map);
        }
    }

    /// Print ticket priority
    fn print_priority(meta: &TicketMetadata) {
        let priority = meta.priority.map(|p| p.as_num()).unwrap_or(2);
        println!("  Priority: P{}", priority);
    }

    /// Print ticket dependencies with their status
    fn print_deps(meta: &TicketMetadata, ticket_map: &HashMap<String, TicketMetadata>) {
        if !meta.deps.is_empty() {
            let deps_with_status: Vec<String> = meta
                .deps
                .iter()
                .map(|dep| {
                    let dep_status = ticket_map
                        .get(dep)
                        .and_then(|t| t.status)
                        .map(|s| format!("[{}]", s))
                        .unwrap_or_else(|| "[missing]".to_string());
                    format!("{} {}", dep, dep_status)
                })
                .collect();
            println!("  Deps: {}", deps_with_status.join(", "));
        }
    }
}
