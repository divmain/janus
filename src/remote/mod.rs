//! Remote issue sync module.
//!
//! This module provides functionality for synchronizing Janus tickets with
//! external issue trackers like GitHub Issues and Linear.

pub mod config;
pub mod github;
pub mod linear;

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{JanusError, Result};
use crate::types::TicketStatus;

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
        // Project key is uppercase letters, number is digits
        parts[0].chars().all(|c| c.is_ascii_uppercase())
            && !parts[0].is_empty()
            && parts[1].chars().all(|c| c.is_ascii_digit())
            && !parts[1].is_empty()
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
            } => write!(f, "github:{}/{}/{}", owner, repo, issue_number),
            RemoteRef::Linear { org, issue_id } => write!(f, "linear:{}/{}", org, issue_id),
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
}

/// Platform-agnostic status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
            RemoteStatus::Custom(s) => write!(f, "{}", s),
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

/// Common interface for remote providers
pub trait RemoteProvider: Send + Sync {
    /// Fetch an issue from the remote platform
    fn fetch_issue(
        &self,
        remote_ref: &RemoteRef,
    ) -> impl std::future::Future<Output = Result<RemoteIssue>> + Send;

    /// Create a new issue on the remote platform
    fn create_issue(
        &self,
        title: &str,
        body: &str,
    ) -> impl std::future::Future<Output = Result<RemoteRef>> + Send;

    /// Update an existing issue on the remote platform
    fn update_issue(
        &self,
        remote_ref: &RemoteRef,
        updates: IssueUpdates,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
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
}
