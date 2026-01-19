//! Filtering logic for remote TUI
//!
//! Provides fuzzy matching across local tickets and remote issues.

use crate::remote::RemoteIssue;
use crate::tui::search::filter_items;
use crate::types::TicketMetadata;

/// A local ticket with its fuzzy match score and matched indices
#[derive(Debug, Clone, Default)]
pub struct FilteredLocalTicket {
    pub ticket: TicketMetadata,
    pub score: i64,
    pub title_indices: Vec<usize>,
}

/// A remote issue with its fuzzy match score and matched indices
#[derive(Debug, Clone)]
pub struct FilteredRemoteIssue {
    pub issue: RemoteIssue,
    pub score: i64,
    pub title_indices: Vec<usize>,
}

/// Filter local tickets by a fuzzy search query
pub fn filter_local_tickets(tickets: &[TicketMetadata], query: &str) -> Vec<FilteredLocalTicket> {
    let results = filter_items(
        tickets,
        query,
        |ticket| {
            format!(
                "{} {} {} {}",
                ticket.id.as_deref().unwrap_or(""),
                ticket.title.as_deref().unwrap_or(""),
                ticket
                    .ticket_type
                    .map(|t| t.to_string())
                    .unwrap_or_default(),
                ticket.status.unwrap_or_default(),
            )
        },
        |ticket| {
            let id_len = ticket.id.as_ref().map(|s| s.len()).unwrap_or(0) + 1;
            let title_len = ticket.title.as_ref().map(|s| s.len()).unwrap_or(0);
            (id_len, title_len)
        },
    );

    // Convert from FilteredItem to FilteredLocalTicket
    results
        .into_iter()
        .map(|filtered| FilteredLocalTicket {
            ticket: filtered.item,
            score: filtered.score,
            title_indices: filtered.title_indices,
        })
        .collect()
}

/// Filter remote issues by a fuzzy search query
pub fn filter_remote_issues(issues: &[RemoteIssue], query: &str) -> Vec<FilteredRemoteIssue> {
    let results = filter_items(
        issues,
        query,
        |issue| {
            format!(
                "{} {} {} {} {} {}",
                issue.id,
                issue.title,
                issue.status,
                issue.labels.join(" "),
                issue.assignee.as_deref().unwrap_or(""),
                issue.team.as_deref().unwrap_or(""),
            )
        },
        |issue| {
            let id_len = issue.id.len() + 1;
            let title_len = issue.title.len();
            (id_len, title_len)
        },
    );

    // Convert from FilteredItem to FilteredRemoteIssue
    results
        .into_iter()
        .map(|filtered| FilteredRemoteIssue {
            issue: filtered.item,
            score: filtered.score,
            title_indices: filtered.title_indices,
        })
        .collect()
}
