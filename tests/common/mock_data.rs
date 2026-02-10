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
                id: Some(TicketId::new_unchecked(id)),
                status: Some(TicketStatus::New),
                ticket_type: Some(TicketType::Task),
                priority: Some(TicketPriority::P2),
                created: Some(CreatedAt::new_unchecked("2024-01-01T00:00:00Z")),
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
        self.metadata.deps.push(dep_id.to_string());
        self
    }

    /// Set the parent ticket
    pub fn parent(mut self, parent_id: &str) -> Self {
        self.metadata.parent = Some(TicketId::new_unchecked(parent_id));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ticket_builder_basic() {
        let ticket = TicketBuilder::new("j-test").build();
        assert_eq!(ticket.id.as_deref(), Some("j-test"));
        assert_eq!(ticket.status, Some(TicketStatus::New));
    }

    #[test]
    fn test_ticket_builder_with_all_fields() {
        let ticket = TicketBuilder::new("j-test")
            .title("Test Title")
            .status(TicketStatus::InProgress)
            .ticket_type(TicketType::Bug)
            .priority(TicketPriority::P0)
            .dep("j-dep1")
            .parent("j-parent")
            .build();

        assert_eq!(ticket.id.as_deref(), Some("j-test"));
        assert_eq!(ticket.title, Some("Test Title".to_string()));
        assert_eq!(ticket.status, Some(TicketStatus::InProgress));
        assert_eq!(ticket.ticket_type, Some(TicketType::Bug));
        assert_eq!(ticket.priority, Some(TicketPriority::P0));
        assert_eq!(ticket.deps, vec!["j-dep1"]);
        assert_eq!(ticket.parent.as_deref(), Some("j-parent"));
    }

    #[test]
    fn test_mock_ticket() {
        let ticket = mock_ticket("j-123", TicketStatus::Complete);
        assert_eq!(ticket.id.as_deref(), Some("j-123"));
        assert_eq!(ticket.status, Some(TicketStatus::Complete));
        assert!(ticket.title.unwrap().contains("j-123"));
    }

    #[test]
    fn test_mock_tickets() {
        let tickets = mock_tickets(&[("j-1", TicketStatus::New), ("j-2", TicketStatus::Complete)]);
        assert_eq!(tickets.len(), 2);
        assert_eq!(tickets[0].status, Some(TicketStatus::New));
        assert_eq!(tickets[1].status, Some(TicketStatus::Complete));
    }

    #[test]
    fn test_remote_issue_builder_basic() {
        let issue = RemoteIssueBuilder::new("GH-123").build();
        assert_eq!(issue.id, "GH-123");
        assert_eq!(issue.status, RemoteStatus::Open);
    }

    #[test]
    fn test_remote_issue_builder_with_all_fields() {
        let issue = RemoteIssueBuilder::new("LIN-456")
            .title("Important bug")
            .body("Description of the bug")
            .status(RemoteStatus::Closed)
            .priority(1)
            .assignee("alice")
            .label("bug")
            .label("priority")
            .team("Engineering")
            .project("Backend")
            .url("https://linear.app/issue/LIN-456")
            .build();

        assert_eq!(issue.id, "LIN-456");
        assert_eq!(issue.title, "Important bug");
        assert_eq!(issue.body, "Description of the bug");
        assert_eq!(issue.status, RemoteStatus::Closed);
        assert_eq!(issue.priority, Some(1));
        assert_eq!(issue.assignee, Some("alice".to_string()));
        assert_eq!(issue.labels, vec!["bug", "priority"]);
        assert_eq!(issue.team, Some("Engineering".to_string()));
        assert_eq!(issue.project, Some("Backend".to_string()));
        assert_eq!(issue.url, "https://linear.app/issue/LIN-456");
    }

    #[test]
    fn test_mock_remote_issue() {
        let issue = mock_remote_issue("GH-1", RemoteStatus::Open);
        assert_eq!(issue.id, "GH-1");
        assert_eq!(issue.status, RemoteStatus::Open);
        assert!(issue.title.contains("GH-1"));
    }

    #[test]
    fn test_mock_remote_issues() {
        let issues =
            mock_remote_issues(&[("GH-1", RemoteStatus::Open), ("GH-2", RemoteStatus::Closed)]);
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].status, RemoteStatus::Open);
        assert_eq!(issues[1].status, RemoteStatus::Closed);
    }

    #[test]
    fn test_ticket_builder_with_remote() {
        let ticket = TicketBuilder::new("j-linked")
            .title("Linked ticket")
            .remote("linear:acme/ENG-123")
            .build();

        assert_eq!(ticket.id.as_deref(), Some("j-linked"));
        assert_eq!(ticket.remote, Some("linear:acme/ENG-123".to_string()));
    }

    #[test]
    fn test_mock_linked_ticket() {
        let ticket =
            mock_linked_ticket("j-lnk1", "github:owner/repo/456", TicketStatus::InProgress);
        assert_eq!(ticket.id.as_deref(), Some("j-lnk1"));
        assert_eq!(ticket.remote, Some("github:owner/repo/456".to_string()));
        assert_eq!(ticket.status, Some(TicketStatus::InProgress));
        assert!(ticket.title.unwrap().contains("j-lnk1"));
    }
}
