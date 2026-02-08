//! Operation handlers for remote TUI

use crate::error::{JanusError, Result};
use crate::remote::config::Platform;
use crate::remote::{RemoteIssue, RemoteProvider, RemoteRef};
use crate::ticket::TicketBuilder;
use crate::types::TicketMetadata;
use std::collections::HashSet;
use thiserror::Error;
use url::Url;

use super::sync_preview::{SyncChange, SyncDirection};

/// Sanitize a string to prevent YAML frontmatter injection
/// Replaces "---" (YAML delimiter) with safe HTML entities
fn sanitize_for_yaml(input: &str) -> String {
    input.replace("---", "&#45;&#45;&#45;")
}

/// Extract the issue ID from a remote reference string
pub fn extract_issue_id_from_remote_ref(remote_ref: &str) -> Option<String> {
    let parts: Vec<&str> = remote_ref.split(':').collect();
    if parts.len() != 2 {
        return None;
    }

    match parts[0] {
        "github" => {
            let id_parts: Vec<&str> = parts[1].split('/').collect();
            if id_parts.len() >= 3 {
                Some(id_parts[2].to_string())
            } else {
                None
            }
        }
        "linear" => {
            let id_parts: Vec<&str> = parts[1].split('/').collect();
            if id_parts.len() >= 2 {
                Some(id_parts[1].to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::RemoteStatus;

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

    #[test]
    fn test_build_remote_ref_from_github_issue_by_number() {
        let issue = RemoteIssue {
            id: "2".to_string(),
            url: "https://github.com/owner/repo/issues/2".to_string(),
            title: "Test Issue".to_string(),
            body: "".to_string(),
            status: RemoteStatus::Open,
            priority: None,
            assignee: None,
            updated_at: "".to_string(),
            labels: vec![],
            team: None,
            project: None,
            milestone: None,
            due_date: None,
            created_at: "".to_string(),
            creator: None,
        };

        let remote_ref = build_remote_ref_from_issue(&issue).unwrap();
        match remote_ref {
            RemoteRef::GitHub {
                owner,
                repo,
                issue_number,
            } => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(issue_number, 2);
            }
            _ => panic!("Expected GitHub ref"),
        }
    }

    #[test]
    fn test_build_remote_ref_from_linear_issue_by_id() {
        let issue = RemoteIssue {
            id: "ENG-123".to_string(),
            url: "https://linear.app/my-org/issue/ENG-123/test-issue".to_string(),
            title: "Test Issue".to_string(),
            body: "".to_string(),
            status: RemoteStatus::Open,
            priority: None,
            assignee: None,
            updated_at: "".to_string(),
            labels: vec![],
            team: None,
            project: None,
            milestone: None,
            due_date: None,
            created_at: "".to_string(),
            creator: None,
        };

        let remote_ref = build_remote_ref_from_issue(&issue).unwrap();
        match remote_ref {
            RemoteRef::Linear { org, issue_id } => {
                assert_eq!(org, "my-org");
                assert_eq!(issue_id, "ENG-123");
            }
            _ => panic!("Expected Linear ref"),
        }
    }

    #[test]
    fn test_build_remote_ref_invalid() {
        let issue = RemoteIssue {
            id: "invalid".to_string(),
            url: "https://example.com/invalid".to_string(),
            title: "Test".to_string(),
            body: "".to_string(),
            status: RemoteStatus::Open,
            priority: None,
            assignee: None,
            updated_at: "".to_string(),
            labels: vec![],
            team: None,
            project: None,
            milestone: None,
            due_date: None,
            created_at: "".to_string(),
            creator: None,
        };

        let result = build_remote_ref_from_issue(&issue);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_issue_id_from_github_ref() {
        assert_eq!(
            extract_issue_id_from_remote_ref("github:owner/repo/123"),
            Some("123".to_string())
        );
        assert_eq!(
            extract_issue_id_from_remote_ref("github:foo/bar/456"),
            Some("456".to_string())
        );
    }

    #[test]
    fn test_extract_issue_id_from_linear_ref() {
        assert_eq!(
            extract_issue_id_from_remote_ref("linear:org/PROJ-123"),
            Some("PROJ-123".to_string())
        );
        assert_eq!(
            extract_issue_id_from_remote_ref("linear:company/ENG-456"),
            Some("ENG-456".to_string())
        );
    }

    #[test]
    fn test_extract_issue_id_invalid_formats() {
        assert_eq!(extract_issue_id_from_remote_ref("invalid:format"), None);
        assert_eq!(extract_issue_id_from_remote_ref("github:owner"), None);
        assert_eq!(extract_issue_id_from_remote_ref("github:owner/repo"), None);
        assert_eq!(extract_issue_id_from_remote_ref("linear:org"), None);
        assert_eq!(extract_issue_id_from_remote_ref(""), None);
    }

    #[test]
    fn test_build_remote_ref_github_with_trailing_slash() {
        let issue = RemoteIssue {
            id: "42".to_string(),
            url: "https://github.com/owner/repo/issues/42/".to_string(),
            title: "Test Issue".to_string(),
            body: "".to_string(),
            status: RemoteStatus::Open,
            priority: None,
            assignee: None,
            updated_at: "".to_string(),
            labels: vec![],
            team: None,
            project: None,
            milestone: None,
            due_date: None,
            created_at: "".to_string(),
            creator: None,
        };

        let remote_ref = build_remote_ref_from_issue(&issue).unwrap();
        match remote_ref {
            RemoteRef::GitHub {
                owner,
                repo,
                issue_number,
            } => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(issue_number, 42);
            }
            _ => panic!("Expected GitHub ref"),
        }
    }

    #[test]
    fn test_build_remote_ref_github_with_www_prefix() {
        let issue = RemoteIssue {
            id: "99".to_string(),
            url: "https://www.github.com/owner/repo/issues/99".to_string(),
            title: "Test Issue".to_string(),
            body: "".to_string(),
            status: RemoteStatus::Open,
            priority: None,
            assignee: None,
            updated_at: "".to_string(),
            labels: vec![],
            team: None,
            project: None,
            milestone: None,
            due_date: None,
            created_at: "".to_string(),
            creator: None,
        };

        let remote_ref = build_remote_ref_from_issue(&issue).unwrap();
        match remote_ref {
            RemoteRef::GitHub {
                owner,
                repo,
                issue_number,
            } => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(issue_number, 99);
            }
            _ => panic!("Expected GitHub ref"),
        }
    }

    #[test]
    fn test_build_remote_ref_linear_with_trailing_slash() {
        let issue = RemoteIssue {
            id: "ENG-456".to_string(),
            url: "https://linear.app/my-org/issue/ENG-456/test-issue/".to_string(),
            title: "Test Issue".to_string(),
            body: "".to_string(),
            status: RemoteStatus::Open,
            priority: None,
            assignee: None,
            updated_at: "".to_string(),
            labels: vec![],
            team: None,
            project: None,
            milestone: None,
            due_date: None,
            created_at: "".to_string(),
            creator: None,
        };

        let remote_ref = build_remote_ref_from_issue(&issue).unwrap();
        match remote_ref {
            RemoteRef::Linear { org, issue_id } => {
                assert_eq!(org, "my-org");
                assert_eq!(issue_id, "ENG-456");
            }
            _ => panic!("Expected Linear ref"),
        }
    }

    #[test]
    fn test_build_remote_ref_github_with_query_params() {
        let issue = RemoteIssue {
            id: "7".to_string(),
            url: "https://github.com/owner/repo/issues/7?q=1".to_string(),
            title: "Test Issue".to_string(),
            body: "".to_string(),
            status: RemoteStatus::Open,
            priority: None,
            assignee: None,
            updated_at: "".to_string(),
            labels: vec![],
            team: None,
            project: None,
            milestone: None,
            due_date: None,
            created_at: "".to_string(),
            creator: None,
        };

        let remote_ref = build_remote_ref_from_issue(&issue).unwrap();
        match remote_ref {
            RemoteRef::GitHub {
                owner,
                repo,
                issue_number,
            } => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(issue_number, 7);
            }
            _ => panic!("Expected GitHub ref"),
        }
    }

    #[test]
    fn test_build_remote_ref_invalid_url() {
        let issue = RemoteIssue {
            id: "123".to_string(),
            url: "not-a-valid-url".to_string(),
            title: "Test".to_string(),
            body: "".to_string(),
            status: RemoteStatus::Open,
            priority: None,
            assignee: None,
            updated_at: "".to_string(),
            labels: vec![],
            team: None,
            project: None,
            milestone: None,
            due_date: None,
            created_at: "".to_string(),
            creator: None,
        };

        let result = build_remote_ref_from_issue(&issue);
        assert!(result.is_err());
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
    let parsed_url = Url::parse(&issue.url)
        .map_err(|e| JanusError::InvalidRemoteRef(issue.id.clone(), format!("invalid URL: {e}")))?;

    let host = parsed_url.host_str().ok_or_else(|| {
        JanusError::InvalidRemoteRef(issue.id.clone(), "URL has no host".to_string())
    })?;

    if host.ends_with("github.com") {
        let mut path_segments = parsed_url.path_segments().ok_or_else(|| {
            JanusError::InvalidRemoteRef(issue.id.clone(), "invalid GitHub URL path".to_string())
        })?;

        let owner = path_segments
            .next()
            .ok_or_else(|| {
                JanusError::InvalidRemoteRef(
                    issue.id.clone(),
                    "missing owner in GitHub URL".to_string(),
                )
            })?
            .to_string();

        let repo = path_segments
            .next()
            .ok_or_else(|| {
                JanusError::InvalidRemoteRef(
                    issue.id.clone(),
                    "missing repo in GitHub URL".to_string(),
                )
            })?
            .to_string();

        let _issues_segment = path_segments.next().ok_or_else(|| {
            JanusError::InvalidRemoteRef(
                issue.id.clone(),
                "missing 'issues' segment in GitHub URL".to_string(),
            )
        })?;

        let issue_number: u64 = issue.id.parse().map_err(|_| {
            JanusError::InvalidRemoteRef(issue.id.clone(), "invalid issue number".to_string())
        })?;

        return Ok(RemoteRef::GitHub {
            owner,
            repo,
            issue_number,
        });
    }

    if host.ends_with("linear.app") {
        let mut path_segments = parsed_url.path_segments().ok_or_else(|| {
            JanusError::InvalidRemoteRef(issue.id.clone(), "invalid Linear URL path".to_string())
        })?;

        let org = path_segments
            .next()
            .ok_or_else(|| {
                JanusError::InvalidRemoteRef(
                    issue.id.clone(),
                    "missing org in Linear URL".to_string(),
                )
            })?
            .to_string();

        let _issue_segment = path_segments.next().ok_or_else(|| {
            JanusError::InvalidRemoteRef(
                issue.id.clone(),
                "missing 'issue' segment in Linear URL".to_string(),
            )
        })?;

        return Ok(RemoteRef::Linear {
            org,
            issue_id: issue.id.clone(),
        });
    }

    Err(JanusError::InvalidRemoteRef(
        issue.id.clone(),
        "unsupported URL host".to_string(),
    ))
}

/// Create a local ticket from a remote issue
fn create_ticket_from_remote(remote_issue: &RemoteIssue, remote_ref: &RemoteRef) -> Result<String> {
    let status = remote_issue.status.to_ticket_status();
    let priority = remote_issue.priority.unwrap_or(2);

    let sanitized_title = sanitize_for_yaml(&remote_issue.title);
    let sanitized_body = sanitize_for_yaml(&remote_issue.body);

    if sanitized_title.is_empty() {
        return Err(JanusError::EmptyTitle);
    }

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
pub fn create_sync_changes(
    ticket: &TicketMetadata,
    issue: &RemoteIssue,
) -> Result<Vec<SyncChange>> {
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

    let local_status = ticket.status.ok_or_else(|| {
        JanusError::Other(format!(
            "Ticket '{}' is missing status field and may be corrupted",
            ticket.id.as_deref().unwrap_or("unknown")
        ))
    })?;
    let remote_status = issue.status.to_ticket_status();
    if local_status != remote_status {
        changes.push(SyncChange {
            field_name: "Status".to_string(),
            local_value: local_status.to_string(),
            remote_value: remote_status.to_string(),
            direction: SyncDirection::LocalToRemote,
        });
    }

    Ok(changes)
}

/// Link a local ticket to a remote issue
pub async fn link_ticket_to_issue(local_ticket_id: &str, remote_issue: &RemoteIssue) -> Result<()> {
    use crate::ticket::Ticket;

    let remote_ref = build_remote_ref_from_issue(remote_issue)?;
    let ticket = Ticket::find(local_ticket_id).await?;

    // Update the ticket's remote field
    ticket.update_field("remote", &remote_ref.to_string())?;

    Ok(())
}

/// Unlink a local ticket from its remote issue
pub async fn unlink_ticket(local_ticket_id: &str) -> Result<()> {
    use crate::ticket::Ticket;

    let ticket = Ticket::find(local_ticket_id).await?;

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
#[derive(Error, Debug)]
pub enum PushError {
    #[error("authentication failed for ticket '{ticket_id}': {message}")]
    Auth {
        ticket_id: String,
        message: String,
        #[source]
        source: Option<JanusError>,
    },

    #[error("API error for ticket '{ticket_id}': {message}")]
    Api {
        ticket_id: String,
        message: String,
        #[source]
        source: Option<JanusError>,
    },

    #[error("rate limit exceeded for ticket '{ticket_id}': retry after {retry_after_seconds}s")]
    RateLimited {
        ticket_id: String,
        retry_after_seconds: u64,
    },

    #[error("network error for ticket '{ticket_id}': {message}")]
    Network {
        ticket_id: String,
        message: String,
        #[source]
        source: Option<reqwest::Error>,
    },

    #[error("configuration error for ticket '{ticket_id}': {message}")]
    Config {
        ticket_id: String,
        message: String,
        #[source]
        source: Option<JanusError>,
    },

    #[error("ticket '{ticket_id}' not found")]
    TicketNotFound {
        ticket_id: String,
        #[source]
        source: Option<JanusError>,
    },

    #[error("failed to read ticket '{ticket_id}': {message}")]
    TicketReadError {
        ticket_id: String,
        message: String,
        #[source]
        source: Option<JanusError>,
    },

    #[error("failed to write ticket '{ticket_id}': {message}")]
    TicketWriteError {
        ticket_id: String,
        message: String,
        #[source]
        source: Option<JanusError>,
    },

    #[error("ticket '{ticket_id}' is already linked to a remote issue")]
    AlreadyLinked { ticket_id: String },
}

impl PushError {
    /// Get the ticket ID associated with this error
    pub fn ticket_id(&self) -> &str {
        match self {
            PushError::Auth { ticket_id, .. }
            | PushError::Api { ticket_id, .. }
            | PushError::RateLimited { ticket_id, .. }
            | PushError::Network { ticket_id, .. }
            | PushError::Config { ticket_id, .. }
            | PushError::TicketNotFound { ticket_id, .. }
            | PushError::TicketReadError { ticket_id, .. }
            | PushError::TicketWriteError { ticket_id, .. }
            | PushError::AlreadyLinked { ticket_id } => ticket_id,
        }
    }

    /// Get the error message for display
    pub fn error_message(&self) -> String {
        match self {
            PushError::Auth { message, .. } => message.clone(),
            PushError::Api { message, .. } => message.clone(),
            PushError::RateLimited {
                retry_after_seconds,
                ..
            } => {
                format!("rate limited, retry after {retry_after_seconds}s")
            }
            PushError::Network { message, .. } => message.clone(),
            PushError::Config { message, .. } => message.clone(),
            PushError::TicketNotFound { .. } => "ticket not found".to_string(),
            PushError::TicketReadError { message, .. } => message.clone(),
            PushError::TicketWriteError { message, .. } => message.clone(),
            PushError::AlreadyLinked { .. } => "already linked to remote issue".to_string(),
        }
    }
}

impl From<(JanusError, String)> for PushError {
    fn from((error, ticket_id): (JanusError, String)) -> Self {
        match &error {
            JanusError::Auth(msg) => PushError::Auth {
                ticket_id,
                message: msg.clone(),
                source: Some(error),
            },
            JanusError::Api(msg) => PushError::Api {
                ticket_id,
                message: msg.clone(),
                source: Some(error),
            },
            JanusError::RateLimited(seconds) => PushError::RateLimited {
                ticket_id,
                retry_after_seconds: *seconds,
            },
            JanusError::Http(_) => PushError::Network {
                ticket_id,
                message: error.to_string(),
                source: None,
            },
            JanusError::Config(msg) => PushError::Config {
                ticket_id,
                message: msg.clone(),
                source: Some(error),
            },
            JanusError::TicketNotFound(_) => PushError::TicketNotFound {
                ticket_id,
                source: Some(error),
            },
            JanusError::NotLinked | JanusError::AlreadyLinked(_) => {
                PushError::AlreadyLinked { ticket_id }
            }
            _ => PushError::TicketReadError {
                ticket_id,
                message: error.to_string(),
                source: Some(error),
            },
        }
    }
}

/// Push a single local ticket to the remote platform
pub async fn push_ticket_to_remote(
    ticket_id: &str,
    platform: Platform,
) -> std::result::Result<PushResult, PushError> {
    use crate::ticket::Ticket;

    // Load config
    let config = crate::remote::config::Config::load()
        .map_err(|e| PushError::from((e, ticket_id.to_string())))?;

    // Find and read the ticket
    let ticket = Ticket::find(ticket_id).await.map_err(|e| match e {
        JanusError::TicketNotFound(_) => PushError::TicketNotFound {
            ticket_id: ticket_id.to_string(),
            source: Some(e),
        },
        _ => PushError::TicketReadError {
            ticket_id: ticket_id.to_string(),
            message: e.to_string(),
            source: Some(e),
        },
    })?;

    let metadata = ticket.read().map_err(|e| PushError::TicketReadError {
        ticket_id: ticket_id.to_string(),
        message: e.to_string(),
        source: Some(e),
    })?;

    // Check if already linked
    if metadata.remote.is_some() {
        return Err(PushError::AlreadyLinked {
            ticket_id: ticket_id.to_string(),
        });
    }

    let title = metadata.title.unwrap_or_else(|| "Untitled".to_string());

    // Read raw content to extract body
    let content = ticket
        .read_content()
        .map_err(|e| PushError::TicketReadError {
            ticket_id: ticket_id.to_string(),
            message: e.to_string(),
            source: Some(e),
        })?;

    use crate::parser;
    use crate::parser::TITLE_RE;

    let (_, body_with_title) =
        parser::split_frontmatter(&content).unwrap_or_else(|_| (String::new(), content.clone()));

    let body = TITLE_RE.replace(&body_with_title, "").to_string();

    // Create the remote issue
    let remote_ref =
        match platform {
            Platform::GitHub => {
                let provider = crate::remote::github::GitHubProvider::from_config(&config)
                    .map_err(|e| PushError::Config {
                        ticket_id: ticket_id.to_string(),
                        message: format!("Failed to create GitHub provider: {e}"),
                        source: Some(e),
                    })?;

                provider
                    .create_issue(&title, &body)
                    .await
                    .map_err(|e| PushError::from((e, ticket_id.to_string())))?
            }
            Platform::Linear => {
                let provider = crate::remote::linear::LinearProvider::from_config(&config)
                    .map_err(|e| PushError::Config {
                        ticket_id: ticket_id.to_string(),
                        message: format!("Failed to create Linear provider: {e}"),
                        source: Some(e),
                    })?;

                provider
                    .create_issue(&title, &body)
                    .await
                    .map_err(|e| PushError::from((e, ticket_id.to_string())))?
            }
        };

    // Update the local ticket with the remote reference
    ticket
        .update_field("remote", &remote_ref.to_string())
        .map_err(|e| PushError::TicketWriteError {
            ticket_id: ticket_id.to_string(),
            message: format!("Failed to update ticket with remote ref: {e}"),
            source: Some(e),
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
) -> Result<Vec<SyncChange>> {
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
    let local_status = ticket.status.ok_or_else(|| {
        JanusError::Other(format!(
            "Ticket '{}' is missing status field and may be corrupted",
            ticket.id.as_deref().unwrap_or("unknown")
        ))
    })?;
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

    Ok(changes)
}

/// Apply a single sync change to a local ticket
pub async fn apply_sync_change_to_local(ticket_id: &str, change: &SyncChange) -> Result<()> {
    use crate::ticket::Ticket;

    let ticket = Ticket::find(ticket_id).await?;

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

use regex::Regex;
use std::sync::LazyLock;

static UPDATE_TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^#\s+.*$").expect("valid regex"));

/// Update the title in ticket content
fn update_title_in_content(content: &str, new_title: &str) -> String {
    UPDATE_TITLE_RE
        .replace(content, format!("# {new_title}"))
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
