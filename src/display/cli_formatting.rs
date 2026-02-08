use crate::types::TicketMetadata;
use owo_colors::OwoColorize;

/// Format a ticket for single-line display with colors
pub fn format_ticket_line(
    ticket: &TicketMetadata,
    options: super::data_formatting::FormatOptions,
) -> String {
    let id = ticket.id.as_deref().unwrap_or("???");
    let id_padded = format!("{id:8}");

    let priority_str = if options.show_priority {
        format!(
            "[P{}]",
            ticket
                .priority
                .map(|p| p.to_string())
                .unwrap_or("2".to_string())
        )
    } else {
        String::new()
    };

    let status = ticket.status.unwrap_or_default();
    let title = ticket.title.as_deref().unwrap_or("");
    let suffix = options.suffix.unwrap_or_default();

    let colored_status = super::format_status_colored(status);

    let colored_id = id_padded.cyan().to_string();

    // Color priority if P0 or P1
    let colored_priority = if options.show_priority {
        match ticket.priority.map(|p| p.as_num()) {
            Some(0) => priority_str.red().to_string(),
            Some(1) => priority_str.yellow().to_string(),
            _ => priority_str,
        }
    } else {
        priority_str
    };

    format!("{colored_id} {colored_priority}{colored_status} - {title}{suffix}")
}

/// Format a ticket as a bullet point (for show command sections) with colors
pub fn format_ticket_bullet(ticket: &TicketMetadata) -> String {
    let id = ticket.id.as_deref().unwrap_or("???");
    let status = ticket.status.unwrap_or_default();
    let title = ticket.title.as_deref().unwrap_or("");
    format!("- {} [{}] {}", id.cyan(), status, title)
}
