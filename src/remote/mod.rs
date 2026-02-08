//! Remote issue sync module.
//!
//! This module provides functionality for synchronizing Janus tickets with
//! external issue trackers like GitHub Issues and Linear.

pub mod config;
pub mod error;
pub mod github;
pub mod linear;

pub use error::{ApiError, build_github_error_message};

use std::fmt;
use std::str::FromStr;

use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};

use crate::error::{JanusError, Result};
use crate::types::TicketStatus;

use crate::remote::github::GitHubProvider;
use crate::remote::linear::LinearProvider;

pub use config::{Config, Platform};

/// Parsed remote reference
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteRef {
    GitHub {
        owner: String,
        repo: String,
        issue_number: u64,
    },
    Linear {
        org: String,
        issue_id: String,
    },
}

impl RemoteRef {
    /// Parse from string like "github:owner/repo/123" or "linear:org/PROJ-123"
    ///
    /// With a config, short formats are also supported:
    /// - "PROJ-123" resolves to "linear:default-org/PROJ-123"
    /// - "owner/repo/123" resolves to "github:owner/repo/123"
    pub fn parse(s: &str, config: Option<&Config>) -> Result<Self> {
        let s = s.trim();

        // Check for platform prefix
        if let Some(rest) = s.strip_prefix("github:") {
            return Self::parse_github_ref(rest);
        }
        if let Some(rest) = s.strip_prefix("linear:") {
            return Self::parse_linear_ref(rest);
        }

        // Try short formats
        // Linear short format: PROJ-123 (uppercase project key + number)
        if Self::looks_like_linear_id(s) {
            if let Some(default) = config.and_then(|c| c.default_remote.as_ref())
                && default.platform == Platform::Linear
            {
                return Ok(RemoteRef::Linear {
                    org: default.org.clone(),
                    issue_id: s.to_string(),
                });
            }
            return Err(JanusError::InvalidRemoteRef(
                s.to_string(),
                "Linear issue ID requires default_remote to be configured".to_string(),
            ));
        }

        // GitHub short format: owner/repo/123
        if let Some(github_ref) = Self::try_parse_github_short(s) {
            return Ok(github_ref);
        }

        Err(JanusError::InvalidRemoteRef(
            s.to_string(),
            "expected format: github:owner/repo/123 or linear:org/ISSUE-123".to_string(),
        ))
    }

    /// Parse GitHub reference: owner/repo/123
    fn parse_github_ref(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 3 {
            return Err(JanusError::InvalidRemoteRef(
                s.to_string(),
                "expected format: owner/repo/issue_number".to_string(),
            ));
        }

        let owner = parts[0].to_string();
        let repo = parts[1].to_string();
        let issue_number: u64 = parts[2].parse().map_err(|_| {
            JanusError::InvalidRemoteRef(
                s.to_string(),
                format!("invalid issue number '{}'", parts[2]),
            )
        })?;

        if owner.is_empty() || repo.is_empty() {
            return Err(JanusError::InvalidRemoteRef(
                s.to_string(),
                "owner and repo cannot be empty".to_string(),
            ));
        }

        Ok(RemoteRef::GitHub {
            owner,
            repo,
            issue_number,
        })
    }

    /// Parse Linear reference: org/ISSUE-123
    fn parse_linear_ref(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(JanusError::InvalidRemoteRef(
                s.to_string(),
                "expected format: org/ISSUE-123".to_string(),
            ));
        }

        let org = parts[0].to_string();
        let issue_id = parts[1].to_string();

        if org.is_empty() || issue_id.is_empty() {
            return Err(JanusError::InvalidRemoteRef(
                s.to_string(),
                "org and issue_id cannot be empty".to_string(),
            ));
        }

        Ok(RemoteRef::Linear { org, issue_id })
    }

    /// Check if string looks like a Linear issue ID (e.g., PROJ-123)
    fn looks_like_linear_id(s: &str) -> bool {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 2 {
            return false;
        }
        let project_key = parts[0];
        let number = parts[1];

        // Project key: at least 2 uppercase letters
        // Number: at least 1 digit, reasonable max to prevent overflow
        project_key.len() >= 2
            && project_key.chars().all(|c| c.is_ascii_uppercase())
            && !number.is_empty()
            && number.len() <= 10 // Prevent overflow
            && number.chars().all(|c| c.is_ascii_digit())
    }

    /// Try to parse as GitHub short format: owner/repo/123
    fn try_parse_github_short(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 3 {
            return None;
        }

        let issue_number: u64 = parts[2].parse().ok()?;
        let owner = parts[0].to_string();
        let repo = parts[1].to_string();

        if owner.is_empty() || repo.is_empty() {
            return None;
        }

        Some(RemoteRef::GitHub {
            owner,
            repo,
            issue_number,
        })
    }

    /// Get the platform for this reference
    pub fn platform(&self) -> Platform {
        match self {
            RemoteRef::GitHub { .. } => Platform::GitHub,
            RemoteRef::Linear { .. } => Platform::Linear,
        }
    }
}

impl fmt::Display for RemoteRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemoteRef::GitHub {
                owner,
                repo,
                issue_number,
            } => write!(f, "github:{owner}/{repo}/{issue_number}"),
            RemoteRef::Linear { org, issue_id } => write!(f, "linear:{org}/{issue_id}"),
        }
    }
}

impl FromStr for RemoteRef {
    type Err = JanusError;

    fn from_str(s: &str) -> Result<Self> {
        RemoteRef::parse(s, None)
    }
}

/// Normalized remote issue data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteIssue {
    /// Platform-specific issue ID
    pub id: String,
    /// Issue title
    pub title: String,
    /// Issue body/description
    pub body: String,
    /// Issue status
    pub status: RemoteStatus,
    /// Priority (0-4, if supported by platform)
    pub priority: Option<u8>,
    /// Assignee name
    pub assignee: Option<String>,
    /// Last updated timestamp (ISO 8601)
    pub updated_at: String,
    /// Web URL to view the issue
    pub url: String,
    /// Labels attached to the issue
    #[serde(default)]
    pub labels: Vec<String>,
    /// Team name (Linear only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    /// Project name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Milestone name (GitHub only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone: Option<String>,
    /// Due date (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    /// Created timestamp (ISO 8601)
    pub created_at: String,
    /// Creator name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
}

/// Platform-agnostic status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum RemoteStatus {
    Open,
    Closed,
    /// For Linear's custom workflow states
    Custom(String),
}

impl RemoteStatus {
    /// Convert to Janus TicketStatus
    pub fn to_ticket_status(&self) -> TicketStatus {
        match self {
            RemoteStatus::Open => TicketStatus::New,
            RemoteStatus::Closed => TicketStatus::Complete,
            RemoteStatus::Custom(s) => {
                let lower = s.to_lowercase();
                // Check for exact matches first (case-insensitive)
                if lower == "done" || lower == "complete" || lower == "closed" {
                    return TicketStatus::Complete;
                }
                if lower == "cancelled" || lower == "canceled" {
                    return TicketStatus::Cancelled;
                }
                if lower == "in progress" || lower == "inprogress" {
                    return TicketStatus::InProgress;
                }
                // Fall back to substring matching for non-exact matches
                if lower.contains("done") || lower.contains("complete") || lower.contains("closed")
                {
                    TicketStatus::Complete
                } else if lower.contains("cancel") {
                    TicketStatus::Cancelled
                } else if lower.contains("progress") {
                    TicketStatus::InProgress
                } else {
                    TicketStatus::New
                }
            }
        }
    }

    /// Create from Janus TicketStatus
    pub fn from_ticket_status(status: TicketStatus) -> Self {
        match status {
            TicketStatus::New => RemoteStatus::Open,
            TicketStatus::Next => RemoteStatus::Open,
            TicketStatus::InProgress => RemoteStatus::Open,
            TicketStatus::Complete => RemoteStatus::Closed,
            TicketStatus::Cancelled => RemoteStatus::Closed,
        }
    }
}

impl fmt::Display for RemoteStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemoteStatus::Open => write!(f, "open"),
            RemoteStatus::Closed => write!(f, "closed"),
            RemoteStatus::Custom(s) => write!(f, "{s}"),
        }
    }
}

/// Updates to apply to a remote issue
#[derive(Debug, Clone, Default)]
pub struct IssueUpdates {
    pub title: Option<String>,
    pub body: Option<String>,
    pub status: Option<RemoteStatus>,
    pub priority: Option<u8>,
    pub assignee: Option<String>,
}

impl IssueUpdates {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.body.is_none()
            && self.status.is_none()
            && self.priority.is_none()
            && self.assignee.is_none()
    }
}

/// Query parameters for listing/filtering remote issues
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RemoteQuery {
    pub search: Option<String>,
    pub assignee: Option<String>,
    pub status: Option<RemoteStatusFilter>,
    pub labels: Option<Vec<String>>,
    pub since: Option<String>,
    pub team: Option<String>,
    pub project: Option<String>,
    pub priority: Option<Vec<u8>>,
    pub cycle: Option<String>,
    pub milestone: Option<String>,
    pub creator: Option<String>,
    pub limit: u32,
    pub cursor: Option<String>,
    pub sort_by: Option<SortField>,
    pub sort_direction: Option<SortDirection>,
}

impl RemoteQuery {
    pub fn new() -> Self {
        Self {
            limit: 100,
            ..Default::default()
        }
    }
}

/// Remote status filter
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoteStatusFilter {
    Open,
    Closed,
    All,
}

/// Sort field for remote issues
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SortField {
    Created,
    #[default]
    Updated,
    Priority,
}

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SortDirection {
    Asc,
    #[default]
    Desc,
}

/// Trait for extracting HTTP error information
pub trait AsHttpError: std::fmt::Display {
    fn as_http_error(&self) -> Option<(reqwest::StatusCode, Option<u64>)>;
    fn is_transient(&self) -> bool;
    fn is_rate_limited(&self) -> bool;
    fn get_retry_after(&self) -> Option<std::time::Duration> {
        if let Some((status, retry_after)) = self.as_http_error()
            && status.as_u16() == 429
        {
            if let Some(seconds) = retry_after {
                return Some(std::time::Duration::from_secs(seconds));
            }
            return Some(std::time::Duration::from_secs(60));
        }
        None
    }
}

/// Retry configuration
pub struct RetryConfig {
    pub max_attempts: u32,
    pub base_delay: std::time::Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: std::time::Duration::from_millis(100),
        }
    }
}

async fn execute_with_retry<T, E, F, Fut>(operation: F) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = std::result::Result<T, E>>,
    E: AsHttpError + Into<JanusError>,
{
    let config = RetryConfig::default();
    let mut errors: Vec<String> = Vec::new();

    for attempt in 0..config.max_attempts {
        let fut = operation();
        match fut.await {
            Ok(result) => return Ok(result),
            Err(e) => {
                let should_retry = if let Some((status, _retry_after)) = e.as_http_error() {
                    attempt < config.max_attempts - 1
                        && (status.as_u16() == 429 || status.is_server_error())
                } else {
                    e.is_transient() && attempt < config.max_attempts - 1
                };

                if !should_retry {
                    return Err(e.into());
                }

                if let Some((status, retry_after)) = e.as_http_error()
                    && status.as_u16() == 429
                {
                    let delay = retry_after.unwrap_or(60);
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                } else {
                    let delay_ms = config.base_delay.as_millis() as u64 * 2u64.pow(attempt);
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }

                errors.push(e.to_string());
            }
        }
    }

    Err(JanusError::RetryFailed {
        attempts: config.max_attempts,
        errors,
    })
}

/// Common interface for remote providers
#[enum_dispatch]
pub trait RemoteProvider: Send + Sync {
    /// Fetch an issue from the remote platform
    fn fetch_issue<'a>(
        &'a self,
        remote_ref: &'a RemoteRef,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<RemoteIssue>> + Send + 'a>>;

    /// Create a new issue on the remote platform
    fn create_issue<'a>(
        &'a self,
        title: &str,
        body: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<RemoteRef>> + Send + 'a>>;

    /// Update an existing issue on the remote platform
    fn update_issue<'a>(
        &'a self,
        remote_ref: &'a RemoteRef,
        updates: IssueUpdates,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>>;

    /// List issues from the remote platform with filtering
    fn list_issues<'a>(
        &'a self,
        query: &'a RemoteQuery,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<RemoteIssue>>> + Send + 'a>>;

    /// Search for issues by text
    fn search_issues<'a>(
        &'a self,
        text: &str,
        limit: u32,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<RemoteIssue>>> + Send + 'a>>;
}

/// Enum wrapping all remote provider implementations
#[enum_dispatch(RemoteProvider)]
pub enum Provider {
    GitHub(GitHubProvider),
    Linear(LinearProvider),
}

/// Create a remote provider instance for the given platform
pub fn create_provider(platform: &Platform, config: &Config) -> Result<Provider> {
    match platform {
        Platform::GitHub => Ok(Provider::GitHub(GitHubProvider::from_config(config)?)),
        Platform::Linear => Ok(Provider::Linear(LinearProvider::from_config(config)?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_full() {
        let r = RemoteRef::parse("github:owner/repo/123", None).unwrap();
        assert_eq!(
            r,
            RemoteRef::GitHub {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
                issue_number: 123
            }
        );
    }

    #[test]
    fn test_parse_github_short() {
        let r = RemoteRef::parse("owner/repo/123", None).unwrap();
        assert_eq!(
            r,
            RemoteRef::GitHub {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
                issue_number: 123
            }
        );
    }

    #[test]
    fn test_parse_linear_full() {
        let r = RemoteRef::parse("linear:myorg/PROJ-123", None).unwrap();
        assert_eq!(
            r,
            RemoteRef::Linear {
                org: "myorg".to_string(),
                issue_id: "PROJ-123".to_string()
            }
        );
    }

    #[test]
    fn test_parse_linear_short_with_config() {
        let mut config = Config::default();
        config.set_default_remote(Platform::Linear, "myorg".to_string(), None);

        let r = RemoteRef::parse("PROJ-123", Some(&config)).unwrap();
        assert_eq!(
            r,
            RemoteRef::Linear {
                org: "myorg".to_string(),
                issue_id: "PROJ-123".to_string()
            }
        );
    }

    #[test]
    fn test_parse_linear_short_without_config() {
        let result = RemoteRef::parse("PROJ-123", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_remote_ref_display() {
        let github = RemoteRef::GitHub {
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            issue_number: 123,
        };
        assert_eq!(github.to_string(), "github:owner/repo/123");

        let linear = RemoteRef::Linear {
            org: "myorg".to_string(),
            issue_id: "PROJ-123".to_string(),
        };
        assert_eq!(linear.to_string(), "linear:myorg/PROJ-123");
    }

    #[test]
    fn test_remote_ref_roundtrip() {
        let original = RemoteRef::GitHub {
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            issue_number: 456,
        };
        let s = original.to_string();
        let parsed = RemoteRef::parse(&s, None).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_looks_like_linear_id() {
        assert!(RemoteRef::looks_like_linear_id("PROJ-123"));
        assert!(RemoteRef::looks_like_linear_id("ABC-1"));
        assert!(!RemoteRef::looks_like_linear_id("proj-123")); // lowercase
        assert!(!RemoteRef::looks_like_linear_id("PROJ123")); // no dash
        assert!(!RemoteRef::looks_like_linear_id("123-ABC")); // wrong order
        assert!(!RemoteRef::looks_like_linear_id("PROJ-")); // no number
        assert!(!RemoteRef::looks_like_linear_id("-123")); // no project
        assert!(!RemoteRef::looks_like_linear_id("A-123")); // single char project key
        assert!(!RemoteRef::looks_like_linear_id("AB-12345678901")); // number too long (>10 digits)
        assert!(RemoteRef::looks_like_linear_id("AB-1234567890")); // exactly 10 digits
    }

    #[test]
    fn test_remote_status_mapping() {
        assert_eq!(RemoteStatus::Open.to_ticket_status(), TicketStatus::New);
        assert_eq!(
            RemoteStatus::Closed.to_ticket_status(),
            TicketStatus::Complete
        );
        assert_eq!(
            RemoteStatus::Custom("Done".to_string()).to_ticket_status(),
            TicketStatus::Complete
        );
        assert_eq!(
            RemoteStatus::Custom("Cancelled".to_string()).to_ticket_status(),
            TicketStatus::Cancelled
        );
        assert_eq!(
            RemoteStatus::Custom("In Progress".to_string()).to_ticket_status(),
            TicketStatus::InProgress
        );
    }

    #[test]
    fn test_parse_invalid() {
        assert!(RemoteRef::parse("invalid", None).is_err());
        assert!(RemoteRef::parse("github:", None).is_err());
        assert!(RemoteRef::parse("github:owner", None).is_err());
        assert!(RemoteRef::parse("github:owner/repo", None).is_err());
        assert!(RemoteRef::parse("github:owner/repo/abc", None).is_err());
        assert!(RemoteRef::parse("linear:", None).is_err());
        assert!(RemoteRef::parse("linear:org", None).is_err());
    }

    #[test]
    fn test_parse_empty_owner_repo() {
        assert!(RemoteRef::parse("github://repo/123", None).is_err());
        assert!(RemoteRef::parse("github:owner//123", None).is_err());
    }

    #[test]
    fn test_parse_empty_org_issue() {
        assert!(RemoteRef::parse("linear:/PROJ-123", None).is_err());
        assert!(RemoteRef::parse("linear:org/", None).is_err());
    }

    #[test]
    fn test_parse_negative_issue_number() {
        assert!(RemoteRef::parse("owner/repo/-1", None).is_err());
    }

    #[test]
    fn test_platform_detection() {
        let github = RemoteRef::parse("github:owner/repo/123", None).unwrap();
        assert_eq!(github.platform(), Platform::GitHub);

        let linear = RemoteRef::parse("linear:org/PROJ-123", None).unwrap();
        assert_eq!(linear.platform(), Platform::Linear);
    }

    #[test]
    fn test_remote_query_default_limit() {
        let query = RemoteQuery::new();
        assert_eq!(query.limit, 100);
    }

    #[test]
    fn test_remote_query_with_filters() {
        let mut query = RemoteQuery::new();
        query.limit = 100;
        query.search = Some("test".to_string());
        query.status = Some(RemoteStatusFilter::Open);

        assert_eq!(query.limit, 100);
        assert_eq!(query.search, Some("test".to_string()));
        assert_eq!(query.status, Some(RemoteStatusFilter::Open));
    }

    #[test]
    fn test_sort_field_defaults() {
        assert_eq!(SortDirection::default(), SortDirection::Desc);
        assert_eq!(SortField::default(), SortField::Updated);
    }

    #[test]
    fn test_issue_updates_empty() {
        let updates = IssueUpdates::default();
        assert!(updates.is_empty());
    }

    #[test]
    fn test_issue_updates_not_empty() {
        let updates = IssueUpdates {
            title: Some("Title".to_string()),
            ..Default::default()
        };
        assert!(!updates.is_empty());
    }

    #[test]
    fn test_issue_updates_multiple_fields() {
        let updates = IssueUpdates {
            title: Some("Title".to_string()),
            body: Some("Body".to_string()),
            status: Some(RemoteStatus::Open),
            priority: Some(1),
            assignee: Some("user@example.com".to_string()),
        };
        assert!(!updates.is_empty());
        assert_eq!(updates.title, Some("Title".to_string()));
        assert_eq!(updates.body, Some("Body".to_string()));
        assert_eq!(updates.status, Some(RemoteStatus::Open));
        assert_eq!(updates.priority, Some(1));
        assert_eq!(updates.assignee, Some("user@example.com".to_string()));
    }

    #[test]
    fn test_remote_status_from_ticket() {
        assert_eq!(
            RemoteStatus::from_ticket_status(TicketStatus::New),
            RemoteStatus::Open
        );
        assert_eq!(
            RemoteStatus::from_ticket_status(TicketStatus::Next),
            RemoteStatus::Open
        );
        assert_eq!(
            RemoteStatus::from_ticket_status(TicketStatus::InProgress),
            RemoteStatus::Open
        );
        assert_eq!(
            RemoteStatus::from_ticket_status(TicketStatus::Complete),
            RemoteStatus::Closed
        );
        assert_eq!(
            RemoteStatus::from_ticket_status(TicketStatus::Cancelled),
            RemoteStatus::Closed
        );
    }

    #[test]
    fn test_parse_large_issue_number() {
        let result = RemoteRef::parse("owner/repo/999999999999999", None);
        assert!(result.is_ok());

        if let Ok(RemoteRef::GitHub { issue_number, .. }) = result {
            assert_eq!(issue_number, 999999999999999);
        } else {
            panic!("Expected GitHub ref");
        }
    }
}
