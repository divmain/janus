//! Filtering logic for remote TUI
//!
//! Provides fuzzy matching across local tickets and remote issues.

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

use crate::remote::RemoteIssue;
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
    if query.is_empty() {
        return tickets
            .iter()
            .map(|t| FilteredLocalTicket {
                ticket: t.clone(),
                score: 0,
                title_indices: vec![],
            })
            .collect();
    }

    let matcher = SkimMatcherV2::default().smart_case();

    tickets
        .iter()
        .filter_map(|ticket| {
            let search_text = format!(
                "{} {} {} {} {}",
                ticket.id.as_deref().unwrap_or(""),
                ticket.title.as_deref().unwrap_or(""),
                ticket
                    .ticket_type
                    .map(|t| t.to_string())
                    .unwrap_or_default(),
                ticket.assignee.as_deref().unwrap_or(""),
                ticket.status.unwrap_or_default(),
            );

            matcher
                .fuzzy_indices(&search_text, query)
                .map(|(score, indices)| {
                    let id_len = ticket.id.as_ref().map(|s| s.len()).unwrap_or(0) + 1;
                    let title_len = ticket.title.as_ref().map(|s| s.len()).unwrap_or(0);

                    let title_indices: Vec<usize> = indices
                        .into_iter()
                        .filter(|&i| i >= id_len && i < id_len + title_len)
                        .map(|i| i - id_len)
                        .collect();

                    FilteredLocalTicket {
                        ticket: ticket.clone(),
                        score,
                        title_indices,
                    }
                })
        })
        .collect()
}

/// Filter remote issues by a fuzzy search query
pub fn filter_remote_issues(issues: &[RemoteIssue], query: &str) -> Vec<FilteredRemoteIssue> {
    if query.is_empty() {
        return issues
            .iter()
            .map(|i| FilteredRemoteIssue {
                issue: i.clone(),
                score: 0,
                title_indices: vec![],
            })
            .collect();
    }

    let matcher = SkimMatcherV2::default().smart_case();

    issues
        .iter()
        .filter_map(|issue| {
            let search_text = format!(
                "{} {} {} {} {} {}",
                issue.id,
                issue.title,
                issue.status,
                issue.labels.join(" "),
                issue.assignee.as_deref().unwrap_or(""),
                issue.team.as_deref().unwrap_or(""),
            );

            matcher
                .fuzzy_indices(&search_text, query)
                .map(|(score, indices)| {
                    let id_len = issue.id.len() + 1;

                    let title_indices: Vec<usize> = indices
                        .into_iter()
                        .filter(|&i| i >= id_len && i < id_len + issue.title.len())
                        .map(|i| i - id_len)
                        .collect();

                    FilteredRemoteIssue {
                        issue: issue.clone(),
                        score,
                        title_indices,
                    }
                })
        })
        .collect()
}
