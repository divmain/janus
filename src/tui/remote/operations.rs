//! Operation handlers for remote TUI

use crate::error::{JanusError, Result};
use crate::remote::config::Platform;
use crate::remote::{RemoteIssue, RemoteProvider, RemoteRef};
use crate::ticket::TicketBuilder;
use crate::types::TicketMetadata;
use std::collections::HashSet;

use super::sync_preview::{SyncChange, SyncDirection};

/// Sanitize a string to prevent YAML frontmatter injection
/// Replaces "---" (YAML delimiter) with safe HTML entities
fn sanitize_for_yaml(input: &str) -> String {
    input.replace("---", "&#45;&#45;&#45;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_for_yaml_basic() {
        assert_eq!(sanitize_for_yaml("Hello world"), "Hello world");
    }

    #[test]
    fn test_sanitize_for_yaml_with_delimiter() {
        assert_eq!(sanitize_for_yaml("---"), "&#45;&#45;&#45;");
    }

    #[test]
    fn test_sanitize_for_yaml_multiple_delimiters() {
        assert_eq!(
            sanitize_for_yaml("---\n---"),
            "&#45;&#45;&#45;\n&#45;&#45;&#45;"
        );
    }

    #[test]
    fn test_sanitize_for_yaml_embedded_delimiter() {
        assert_eq!(
            sanitize_for_yaml("Some text\n---\nMore text"),
            "Some text\n&#45;&#45;&#45;\nMore text"
        );
    }

    #[test]
    fn test_sanitize_for_yaml_title_injection() {
        let title = "--- malicious title ---";
        let result = sanitize_for_yaml(title);
        assert!(result.contains("&#45;&#45;&#45;"));
        assert!(!result.contains("---"));
    }

    #[test]
    fn test_sanitize_for_yaml_body_injection() {
        let body = "Description\n---\nid: inject\n---\nMore text";
        let result = sanitize_for_yaml(body);
        assert!(!result.contains("---"));
        assert_eq!(
            result,
            "Description\n&#45;&#45;&#45;\nid: inject\n&#45;&#45;&#45;\nMore text"
        );
    }
}

/// Adopt remote issues into local tickets
pub fn adopt_issues(issues: &[RemoteIssue], _local_ids: &HashSet<String>) -> Result<Vec<String>> {
    let mut adopted_ids = Vec::new();

    for issue in issues {
        let remote_ref = build_remote_ref_from_issue(issue)?;

        let ticket_id = create_ticket_from_remote(issue, &remote_ref)?;
        adopted_ids.push(ticket_id);
    }

    Ok(adopted_ids)
}

/// Build a RemoteRef from a RemoteIssue
fn build_remote_ref_from_issue(issue: &RemoteIssue) -> Result<RemoteRef> {
    // Note: Config might be needed in the future to determine default provider org info
    let _config = crate::remote::config::Config::load()?;

    if issue.id.contains('/') {
        let parts: Vec<&str> = issue.id.split('/').collect();
        if parts.len() == 2 {
            return Ok(RemoteRef::Linear {
                org: parts[0].to_string(),
                issue_id: parts[1].to_string(),
            });
        } else if parts.len() == 3 {
            let issue_number: u64 = parts[2].parse().map_err(|_| {
                JanusError::InvalidRemoteRef(issue.id.clone(), "invalid issue number".to_string())
            })?;
            return Ok(RemoteRef::GitHub {
                owner: parts[0].to_string(),
                repo: parts[1].to_string(),
                issue_number,
            });
        }
    }

    Err(JanusError::InvalidRemoteRef(
        issue.id.clone(),
        "unable to parse".to_string(),
    ))
}

/// Create a local ticket from a remote issue
fn create_ticket_from_remote(remote_issue: &RemoteIssue, remote_ref: &RemoteRef) -> Result<String> {
    let status = remote_issue.status.to_ticket_status();
    let priority = remote_issue.priority.unwrap_or(2);

    let sanitized_title = sanitize_for_yaml(&remote_issue.title);
    let sanitized_body = sanitize_for_yaml(&remote_issue.body);

    let body = if sanitized_body.is_empty() {
        None
    } else {
        Some(sanitized_body)
    };

    let (id, _path) = TicketBuilder::new(&sanitized_title)
        .description(body)
        .status(status.to_string())
        .ticket_type("task")
        .priority(priority.to_string())
        .remote(Some(remote_ref.to_string()))
        .include_uuid(false)
        .run_hooks(false)
        .build()?;

    Ok(id)
}

/// Check for link conflicts - returns issues that are already linked
#[allow(dead_code)]
pub fn check_link_conflicts<'a>(
    issue_ids: &[&'a str],
    local_ids: &HashSet<String>,
) -> Vec<&'a str> {
    issue_ids
        .iter()
        .filter(|&&id| local_ids.contains(id))
        .copied()
        .collect()
}

/// Create sync changes between a local ticket and remote issue
#[allow(dead_code)]
pub fn create_sync_changes(ticket: &TicketMetadata, issue: &RemoteIssue) -> Vec<SyncChange> {
    let mut changes = Vec::new();

    let local_title = ticket.title.as_deref().unwrap_or("");
    if local_title != issue.title {
        changes.push(SyncChange {
            field_name: "Title".to_string(),
            local_value: local_title.to_string(),
            remote_value: issue.title.clone(),
            direction: SyncDirection::LocalToRemote,
        });
    }

    let local_status = ticket.status.unwrap_or_default();
    let remote_status = issue.status.to_ticket_status();
    if local_status != remote_status {
        changes.push(SyncChange {
            field_name: "Status".to_string(),
            local_value: local_status.to_string(),
            remote_value: remote_status.to_string(),
            direction: SyncDirection::LocalToRemote,
        });
    }

    changes
}

/// Link a local ticket to a remote issue
pub fn link_ticket_to_issue(local_ticket_id: &str, remote_issue: &RemoteIssue) -> Result<()> {
    use crate::ticket::Ticket;

    let remote_ref = build_remote_ref_from_issue(remote_issue)?;
    let rt = tokio::runtime::Handle::current();
    let ticket = rt.block_on(Ticket::find(local_ticket_id))?;

    // Update the ticket's remote field
    ticket.update_field("remote", &remote_ref.to_string())?;

    Ok(())
}

/// Unlink a local ticket from its remote issue
pub fn unlink_ticket(local_ticket_id: &str) -> Result<()> {
    use crate::ticket::Ticket;

    let rt = tokio::runtime::Handle::current();
    let ticket = rt.block_on(Ticket::find(local_ticket_id))?;

    // Remove the remote field by setting it to empty
    ticket.remove_field("remote")?;

    Ok(())
}

/// Get tickets that have remote links
#[allow(dead_code)]
pub fn get_linked_ticket_ids(tickets: &[TicketMetadata]) -> HashSet<String> {
    tickets
        .iter()
        .filter(|t| t.remote.is_some())
        .filter_map(|t| t.id.clone())
        .collect()
}

/// Result of a push operation
#[derive(Debug, Clone)]
pub struct PushResult {
    pub ticket_id: String,
    pub remote_ref: RemoteRef,
}

/// Error info for push operation
#[derive(Debug, Clone)]
pub struct PushError {
    pub ticket_id: String,
    pub error: String,
}

/// Extract body content from ticket file content (everything after the title)
fn extract_body_from_content(content: &str) -> String {
    use regex::Regex;

    // Match frontmatter and extract body
    let frontmatter_re =
        Regex::new(r"(?s)^---\n.*?\n---\n(.*)$").expect("frontmatter regex should be valid");

    if let Some(captures) = frontmatter_re.captures(content) {
        let body = captures.get(1).map(|m| m.as_str()).unwrap_or("");

        // Remove the title line (# heading) and get the rest
        let title_re = Regex::new(r"(?m)^#\s+.*$").expect("title regex should be valid");
        let body_without_title = title_re.replace(body, "").to_string();

        // Trim leading/trailing whitespace
        body_without_title.trim().to_string()
    } else {
        String::new()
    }
}

/// Push a single local ticket to the remote platform
pub async fn push_ticket_to_remote(
    ticket_id: &str,
    platform: Platform,
) -> std::result::Result<PushResult, PushError> {
    use crate::ticket::Ticket;

    // Load config
    let config = crate::remote::config::Config::load().map_err(|e| PushError {
        ticket_id: ticket_id.to_string(),
        error: format!("Failed to load config: {}", e),
    })?;

    // Find and read the ticket
    let ticket = Ticket::find(ticket_id).await.map_err(|e| PushError {
        ticket_id: ticket_id.to_string(),
        error: format!("Failed to find ticket: {}", e),
    })?;

    let metadata = ticket.read().map_err(|e| PushError {
        ticket_id: ticket_id.to_string(),
        error: format!("Failed to read ticket: {}", e),
    })?;

    // Check if already linked
    if metadata.remote.is_some() {
        return Err(PushError {
            ticket_id: ticket_id.to_string(),
            error: "Ticket is already linked to a remote issue".to_string(),
        });
    }

    let title = metadata.title.unwrap_or_else(|| "Untitled".to_string());

    // Read raw content to extract body
    let content = ticket.read_content().map_err(|e| PushError {
        ticket_id: ticket_id.to_string(),
        error: format!("Failed to read ticket content: {}", e),
    })?;
    let body = extract_body_from_content(&content);

    // Create the remote issue
    let remote_ref =
        match platform {
            Platform::GitHub => {
                let provider = crate::remote::github::GitHubProvider::from_config(&config)
                    .map_err(|e| PushError {
                        ticket_id: ticket_id.to_string(),
                        error: format!("Failed to create GitHub provider: {}", e),
                    })?;

                provider
                    .create_issue(&title, &body)
                    .await
                    .map_err(|e| PushError {
                        ticket_id: ticket_id.to_string(),
                        error: format!("Failed to create GitHub issue: {}", e),
                    })?
            }
            Platform::Linear => {
                let provider = crate::remote::linear::LinearProvider::from_config(&config)
                    .map_err(|e| PushError {
                        ticket_id: ticket_id.to_string(),
                        error: format!("Failed to create Linear provider: {}", e),
                    })?;

                provider
                    .create_issue(&title, &body)
                    .await
                    .map_err(|e| PushError {
                        ticket_id: ticket_id.to_string(),
                        error: format!("Failed to create Linear issue: {}", e),
                    })?
            }
        };

    // Update the local ticket with the remote reference
    ticket
        .update_field("remote", &remote_ref.to_string())
        .map_err(|e| PushError {
            ticket_id: ticket_id.to_string(),
            error: format!("Failed to update ticket with remote ref: {}", e),
        })?;

    Ok(PushResult {
        ticket_id: ticket_id.to_string(),
        remote_ref,
    })
}

/// Push multiple tickets to remote - returns successes and errors
pub async fn push_tickets_to_remote(
    ticket_ids: &[String],
    platform: Platform,
) -> (Vec<PushResult>, Vec<PushError>) {
    use futures::stream::{self, StreamExt};

    // Clone ticket_ids to owned Strings to avoid lifetime issues
    let owned_ids: Vec<String> = ticket_ids.to_vec();

    let results: Vec<_> = stream::iter(owned_ids)
        .map(|ticket_id| async move { push_ticket_to_remote(&ticket_id, platform).await })
        .buffer_unordered(5) // 5 concurrent pushes
        .collect()
        .await;

    // Separate successes and errors
    let mut successes = Vec::new();
    let mut errors = Vec::new();

    for result in results {
        match result {
            Ok(push_result) => successes.push(push_result),
            Err(push_error) => errors.push(push_error),
        }
    }

    (successes, errors)
}

/// Fetch remote issue data for a linked ticket
pub async fn fetch_remote_issue_for_ticket(
    ticket_id: &str,
    platform: Platform,
) -> Result<(TicketMetadata, crate::remote::RemoteIssue)> {
    use crate::ticket::Ticket;

    let config = crate::remote::config::Config::load()?;
    let ticket = Ticket::find(ticket_id).await?;
    let metadata = ticket.read()?;

    let remote_str = metadata
        .remote
        .as_ref()
        .ok_or_else(|| JanusError::Other("Ticket is not linked to a remote issue".to_string()))?;

    let remote_ref = RemoteRef::parse(remote_str, Some(&config))?;

    let issue = match platform {
        Platform::GitHub => {
            let provider = crate::remote::github::GitHubProvider::from_config(&config)?;
            provider.fetch_issue(&remote_ref).await?
        }
        Platform::Linear => {
            let provider = crate::remote::linear::LinearProvider::from_config(&config)?;
            provider.fetch_issue(&remote_ref).await?
        }
    };

    Ok((metadata, issue))
}

/// Build sync changes for a ticket
pub fn build_sync_changes(
    ticket: &TicketMetadata,
    issue: &crate::remote::RemoteIssue,
) -> Vec<SyncChange> {
    let mut changes = Vec::new();

    // Compare title
    let local_title = ticket.title.as_deref().unwrap_or("");
    if local_title != issue.title {
        changes.push(SyncChange {
            field_name: "Title".to_string(),
            local_value: local_title.to_string(),
            remote_value: issue.title.clone(),
            direction: SyncDirection::RemoteToLocal,
        });
    }

    // Compare status
    let local_status = ticket.status.unwrap_or_default();
    let remote_status = issue.status.to_ticket_status();
    if local_status != remote_status {
        changes.push(SyncChange {
            field_name: "Status".to_string(),
            local_value: local_status.to_string(),
            remote_value: remote_status.to_string(),
            direction: SyncDirection::RemoteToLocal,
        });
    }

    // Compare priority (if remote has one)
    if let Some(remote_priority) = issue.priority {
        let local_priority = ticket.priority.map(|p| p.as_num()).unwrap_or(2);
        if local_priority != remote_priority {
            changes.push(SyncChange {
                field_name: "Priority".to_string(),
                local_value: local_priority.to_string(),
                remote_value: remote_priority.to_string(),
                direction: SyncDirection::RemoteToLocal,
            });
        }
    }

    changes
}

/// Apply a single sync change to a local ticket
pub fn apply_sync_change_to_local(ticket_id: &str, change: &SyncChange) -> Result<()> {
    use crate::ticket::Ticket;

    let rt = tokio::runtime::Handle::current();
    let ticket = rt.block_on(Ticket::find(ticket_id))?;

    match change.field_name.as_str() {
        "Title" => {
            // Title is in the markdown body, not frontmatter
            // We need to update the markdown header
            let content = ticket.read_content()?;
            let new_content = update_title_in_content(&content, &change.remote_value);
            ticket.write(&new_content)?;
        }
        "Status" => {
            ticket.update_field("status", &change.remote_value)?;
        }
        "Priority" => {
            ticket.update_field("priority", &change.remote_value)?;
        }
        _ => {
            return Err(JanusError::Other(format!(
                "Unknown field: {}",
                change.field_name
            )));
        }
    }

    Ok(())
}

/// Update the title in ticket content
fn update_title_in_content(content: &str, new_title: &str) -> String {
    use regex::Regex;

    let title_re = Regex::new(r"(?m)^#\s+.*$").expect("title regex should be valid");
    title_re
        .replace(content, format!("# {}", new_title))
        .to_string()
}

/// Apply a single sync change to a remote issue
pub async fn apply_sync_change_to_remote(
    remote_ref: &str,
    change: &SyncChange,
    platform: Platform,
) -> Result<()> {
    let config = crate::remote::config::Config::load()?;
    let remote_ref = RemoteRef::parse(remote_ref, Some(&config))?;

    let updates = match change.field_name.as_str() {
        "Title" => crate::remote::IssueUpdates {
            title: Some(change.local_value.clone()),
            ..Default::default()
        },
        "Status" => {
            let status = if change.local_value == "complete" || change.local_value == "cancelled" {
                crate::remote::RemoteStatus::Closed
            } else {
                crate::remote::RemoteStatus::Open
            };
            crate::remote::IssueUpdates {
                status: Some(status),
                ..Default::default()
            }
        }
        _ => {
            return Err(JanusError::Other(format!(
                "Cannot sync field '{}' to remote",
                change.field_name
            )));
        }
    };

    match platform {
        Platform::GitHub => {
            let provider = crate::remote::github::GitHubProvider::from_config(&config)?;
            provider.update_issue(&remote_ref, updates).await?;
        }
        Platform::Linear => {
            let provider = crate::remote::linear::LinearProvider::from_config(&config)?;
            provider.update_issue(&remote_ref, updates).await?;
        }
    }

    Ok(())
}
