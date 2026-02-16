//! Mock data builders for creating test tickets and other entities.
//!
//! This module provides builder patterns for creating test data without
//! needing to read from disk.

use janus::remote::{RemoteIssue, RemoteStatus};
use janus::types::{CreatedAt, TicketId, TicketMetadata, TicketPriority, TicketStatus, TicketType};

/// Builder for creating test tickets
pub struct TicketBuilder {
    metadata: TicketMetadata,
}

impl TicketBuilder {
    /// Create a new ticket builder with the given ID
    pub fn new(id: &str) -> Self {
        Self {
            metadata: TicketMetadata {
                id: Some(TicketId::new(id).expect("test id should be valid")),
                status: Some(TicketStatus::New),
                ticket_type: Some(TicketType::Task),
                priority: Some(TicketPriority::P2),
                created: Some(
                    CreatedAt::new("2024-01-01T00:00:00Z").expect("test timestamp should be valid"),
                ),
                ..Default::default()
            },
        }
    }

    /// Set the ticket title
    pub fn title(mut self, title: &str) -> Self {
        self.metadata.title = Some(title.to_string());
        self
    }

    /// Set the ticket status
    pub fn status(mut self, status: TicketStatus) -> Self {
        self.metadata.status = Some(status);
        self
    }

    /// Set the ticket type
    pub fn ticket_type(mut self, t: TicketType) -> Self {
        self.metadata.ticket_type = Some(t);
        self
    }

    /// Set the ticket priority
    pub fn priority(mut self, p: TicketPriority) -> Self {
        self.metadata.priority = Some(p);
        self
    }

    /// Add a dependency
    pub fn dep(mut self, dep_id: &str) -> Self {
        self.metadata
            .deps
            .push(TicketId::new(dep_id).expect("test dep id should be valid"));
        self
    }

    /// Set the parent ticket
    pub fn parent(mut self, parent_id: &str) -> Self {
        self.metadata.parent =
            Some(TicketId::new(parent_id).expect("test parent id should be valid"));
        self
    }

    /// Set the remote reference (e.g., "linear:org/ISSUE-123" or "github:owner/repo/456")
    pub fn remote(mut self, remote: &str) -> Self {
        self.metadata.remote = Some(remote.to_string());
        self
    }

    /// Build the ticket metadata
    pub fn build(self) -> TicketMetadata {
        self.metadata
    }
}

/// Create a basic ticket with minimal setup
pub fn mock_ticket(id: &str, status: TicketStatus) -> TicketMetadata {
    TicketBuilder::new(id)
        .title(&format!("Test ticket {id}"))
        .status(status)
        .build()
}

/// Create multiple tickets with the given statuses
pub fn mock_tickets(specs: &[(&str, TicketStatus)]) -> Vec<TicketMetadata> {
    specs
        .iter()
        .map(|(id, status)| mock_ticket(id, *status))
        .collect()
}

/// Builder for creating test remote issues
pub struct RemoteIssueBuilder {
    issue: RemoteIssue,
}

impl RemoteIssueBuilder {
    /// Create a new remote issue builder with the given ID
    pub fn new(id: &str) -> Self {
        Self {
            issue: RemoteIssue {
                id: id.to_string(),
                title: format!("Test issue {id}"),
                body: String::new(),
                status: RemoteStatus::Open,
                priority: None,
                assignee: None,
                updated_at: "2024-01-01T00:00:00Z".to_string(),
                url: format!("https://example.com/issues/{id}"),
                labels: vec![],
                team: None,
                project: None,
                milestone: None,
                due_date: None,
                created_at: "2024-01-01T00:00:00Z".to_string(),
                creator: None,
            },
        }
    }

    /// Set the issue title
    pub fn title(mut self, title: &str) -> Self {
        self.issue.title = title.to_string();
        self
    }

    /// Set the issue body
    pub fn body(mut self, body: &str) -> Self {
        self.issue.body = body.to_string();
        self
    }

    /// Set the issue status
    pub fn status(mut self, status: RemoteStatus) -> Self {
        self.issue.status = status;
        self
    }

    /// Set the issue priority
    pub fn priority(mut self, priority: u8) -> Self {
        self.issue.priority = Some(priority);
        self
    }

    /// Set the assignee
    pub fn assignee(mut self, assignee: &str) -> Self {
        self.issue.assignee = Some(assignee.to_string());
        self
    }

    /// Add a label
    pub fn label(mut self, label: &str) -> Self {
        self.issue.labels.push(label.to_string());
        self
    }

    /// Set the team
    pub fn team(mut self, team: &str) -> Self {
        self.issue.team = Some(team.to_string());
        self
    }

    /// Set the project
    pub fn project(mut self, project: &str) -> Self {
        self.issue.project = Some(project.to_string());
        self
    }

    /// Set the URL
    pub fn url(mut self, url: &str) -> Self {
        self.issue.url = url.to_string();
        self
    }

    /// Build the remote issue
    pub fn build(self) -> RemoteIssue {
        self.issue
    }
}

/// Create a basic remote issue with minimal setup
pub fn mock_remote_issue(id: &str, status: RemoteStatus) -> RemoteIssue {
    RemoteIssueBuilder::new(id)
        .title(&format!("Test issue {id}"))
        .status(status)
        .build()
}

/// Create multiple remote issues
pub fn mock_remote_issues(specs: &[(&str, RemoteStatus)]) -> Vec<RemoteIssue> {
    specs
        .iter()
        .map(|(id, status)| mock_remote_issue(id, status.clone()))
        .collect()
}

/// Create a ticket that is linked to a remote issue
pub fn mock_linked_ticket(id: &str, remote_ref: &str, status: TicketStatus) -> TicketMetadata {
    TicketBuilder::new(id)
        .title(&format!("Linked ticket {id}"))
        .status(status)
        .remote(remote_ref)
        .build()
}

// Note: Self-tests for this module have been intentionally removed.
// This module is included via #[path] into every test binary, so any tests
// here would be duplicated 10+ times across all test binaries.
