use crate::types::TicketStatus;
use owo_colors::OwoColorize;

pub mod cli_formatting;
pub mod data_formatting;

pub use cli_formatting::*;
pub use data_formatting::*;

pub fn format_status_colored(status: TicketStatus) -> String {
    format_status_colored_with_format(status, |s| format!("[{}]", s))
}

pub fn format_status_colored_with_format<F>(status: TicketStatus, format_fn: F) -> String
where
    F: Fn(&str) -> String,
{
    let badge = format_fn(&status.to_string());
    match status {
        TicketStatus::New => badge.yellow().to_string(),
        TicketStatus::Next => badge.magenta().to_string(),
        TicketStatus::InProgress => badge.cyan().to_string(),
        TicketStatus::Complete => badge.green().to_string(),
        TicketStatus::Cancelled => badge.dimmed().to_string(),
    }
}
