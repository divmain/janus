//! GitHub Issues provider implementation.

use octocrab::Octocrab;

use crate::error::{JanusError, Result};

use super::{
    Config, IssueUpdates, RemoteIssue, RemoteProvider, RemoteQuery, RemoteRef, RemoteStatus,
};

/// GitHub Issues provider
pub struct GitHubProvider {
    client: Octocrab,
    /// Default owner for creating issues
    default_owner: Option<String>,
    /// Default repo for creating issues
    default_repo: Option<String>,
}

impl GitHubProvider {
    /// Create a new GitHub provider from configuration
    pub fn from_config(config: &Config) -> Result<Self> {
        let token = config.github_token().ok_or_else(|| {
            JanusError::Auth(
                "GitHub token not configured. Set GITHUB_TOKEN environment variable or run: janus config set github.token <token>".to_string()
            )
        })?;

        let client = Octocrab::builder()
            .personal_token(token)
            .build()
            .map_err(|e| JanusError::Api(format!("Failed to create GitHub client: {}", e)))?;

        let (default_owner, default_repo) = if let Some(ref default) = config.default_remote {
            if default.platform == super::Platform::GitHub {
                (Some(default.org.clone()), default.repo.clone())
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        Ok(Self {
            client,
            default_owner,
            default_repo,
        })
    }

    /// Create a new GitHub provider with a token
    pub fn new(token: &str) -> Result<Self> {
        let client = Octocrab::builder()
            .personal_token(token.to_string())
            .build()
            .map_err(|e| JanusError::Api(format!("Failed to create GitHub client: {}", e)))?;

        Ok(Self {
            client,
            default_owner: None,
            default_repo: None,
        })
    }

    /// Set default owner and repo for creating issues
    pub fn with_defaults(mut self, owner: String, repo: String) -> Self {
        self.default_owner = Some(owner);
        self.default_repo = Some(repo);
        self
    }
}

impl RemoteProvider for GitHubProvider {
    async fn fetch_issue(&self, remote_ref: &RemoteRef) -> Result<RemoteIssue> {
        let (owner, repo, issue_number) = match remote_ref {
            RemoteRef::GitHub {
                owner,
                repo,
                issue_number,
            } => (owner.as_str(), repo.as_str(), *issue_number),
            _ => {
                return Err(JanusError::Api(
                    "GitHubProvider can only fetch GitHub issues".to_string(),
                ));
            }
        };

        let issue = self
            .client
            .issues(owner, repo)
            .get(issue_number)
            .await
            .map_err(|e| {
                if e.to_string().contains("404") {
                    JanusError::RemoteIssueNotFound(remote_ref.to_string())
                } else {
                    JanusError::Api(format!("GitHub API error: {}", e))
                }
            })?;

        let status = match issue.state {
            octocrab::models::IssueState::Open => RemoteStatus::Open,
            octocrab::models::IssueState::Closed => RemoteStatus::Closed,
            _ => RemoteStatus::Custom(format!("{:?}", issue.state)),
        };

        let labels: Vec<String> = issue.labels.iter().map(|l| l.name.clone()).collect();

        Ok(RemoteIssue {
            id: issue.number.to_string(),
            title: issue.title.clone(),
            body: issue.body.clone().unwrap_or_default(),
            status,
            priority: None,
            assignee: issue.assignee.as_ref().map(|a| a.login.clone()),
            updated_at: issue.updated_at.to_rfc3339(),
            url: issue.html_url.to_string(),
            labels,
            team: None,
            project: None,
            milestone: issue.milestone.as_ref().map(|m| m.title.clone()),
            due_date: None,
            created_at: issue.created_at.to_rfc3339(),
            creator: Some(issue.user.login.clone()),
        })
    }

    async fn create_issue(&self, title: &str, body: &str) -> Result<RemoteRef> {
        let owner = self.default_owner.as_ref().ok_or_else(|| {
            JanusError::Config(
                "No default GitHub owner configured. Set default_remote in config.".to_string(),
            )
        })?;

        let repo = self.default_repo.as_ref().ok_or_else(|| {
            JanusError::Config(
                "No default GitHub repo configured. Set default_remote.repo in config.".to_string(),
            )
        })?;

        let issue = self
            .client
            .issues(owner, repo)
            .create(title)
            .body(body)
            .send()
            .await
            .map_err(|e| JanusError::Api(format!("Failed to create GitHub issue: {}", e)))?;

        Ok(RemoteRef::GitHub {
            owner: owner.clone(),
            repo: repo.clone(),
            issue_number: issue.number,
        })
    }

    async fn update_issue(&self, remote_ref: &RemoteRef, updates: IssueUpdates) -> Result<()> {
        let (owner, repo, issue_number) = match remote_ref {
            RemoteRef::GitHub {
                owner,
                repo,
                issue_number,
            } => (owner.as_str(), repo.as_str(), *issue_number),
            _ => {
                return Err(JanusError::Api(
                    "GitHubProvider can only update GitHub issues".to_string(),
                ));
            }
        };

        // Extract values first to avoid borrow issues with the builder pattern
        let title = updates.title;
        let body = updates.body;
        let status = updates.status;

        // Create the update handler
        let issues_handler = self.client.issues(owner, repo);

        // Build the update - we need to chain all updates together
        // to avoid the borrow checker issues
        let update_builder = issues_handler.update(issue_number);

        // Determine state
        let state = status.map(|s| match s {
            RemoteStatus::Open => octocrab::models::IssueState::Open,
            RemoteStatus::Closed | RemoteStatus::Custom(_) => octocrab::models::IssueState::Closed,
        });

        // Apply all updates at once using the builder
        let update = match (&title, &body, &state) {
            (Some(t), Some(b), Some(s)) => update_builder.title(t).body(b).state(s.clone()),
            (Some(t), Some(b), None) => update_builder.title(t).body(b),
            (Some(t), None, Some(s)) => update_builder.title(t).state(s.clone()),
            (None, Some(b), Some(s)) => update_builder.body(b).state(s.clone()),
            (Some(t), None, None) => update_builder.title(t),
            (None, Some(b), None) => update_builder.body(b),
            (None, None, Some(s)) => update_builder.state(s.clone()),
            (None, None, None) => return Ok(()), // Nothing to update
        };

        update
            .send()
            .await
            .map_err(|e| JanusError::Api(format!("Failed to update GitHub issue: {}", e)))?;

        Ok(())
    }

    async fn list_issues(
        &self,
        query: &RemoteQuery,
    ) -> std::result::Result<Vec<RemoteIssue>, crate::error::JanusError> {
        let owner = self.default_owner.as_ref().ok_or_else(|| {
            JanusError::Config(
                "No default GitHub owner configured. Set default_remote in config.".to_string(),
            )
        })?;

        let repo = self.default_repo.as_ref().ok_or_else(|| {
            JanusError::Config(
                "No default GitHub repo configured. Set default_remote.repo in config.".to_string(),
            )
        })?;

        let issues_handler = self.client.issues(owner, repo);
        let result = issues_handler
            .list()
            .per_page(query.limit as u8)
            .send()
            .await
            .map_err(|e| JanusError::Api(format!("Failed to list GitHub issues: {}", e)))?;

        let issues: Vec<RemoteIssue> = result
            .items
            .into_iter()
            .map(|issue| self.convert_github_issue(&issue))
            .collect();

        Ok(issues)
    }

    async fn search_issues(
        &self,
        text: &str,
        limit: u32,
    ) -> std::result::Result<Vec<RemoteIssue>, crate::error::JanusError> {
        let owner = self.default_owner.as_ref().ok_or_else(|| {
            JanusError::Config(
                "No default GitHub owner configured. Set default_remote in config.".to_string(),
            )
        })?;

        let repo = self.default_repo.as_ref().ok_or_else(|| {
            JanusError::Config(
                "No default GitHub repo configured. Set default_remote.repo in config.".to_string(),
            )
        })?;

        // Build search query: search in the specific repo for issues containing the text
        let query = format!("repo:{}/{} is:issue {}", owner, repo, text);

        let result = self
            .client
            .search()
            .issues_and_pull_requests(&query)
            .per_page(limit.min(100) as u8) // GitHub limits to 100 per page
            .send()
            .await
            .map_err(|e| JanusError::Api(format!("GitHub search failed: {}", e)))?;

        let issues: Vec<RemoteIssue> = result
            .items
            .into_iter()
            .filter(|item| item.pull_request.is_none()) // Filter out PRs
            .map(|issue| RemoteIssue {
                id: issue.number.to_string(),
                title: issue.title.clone(),
                body: issue.body.clone().unwrap_or_default(),
                status: match issue.state {
                    octocrab::models::IssueState::Open => RemoteStatus::Open,
                    octocrab::models::IssueState::Closed => RemoteStatus::Closed,
                    _ => RemoteStatus::Custom(format!("{:?}", issue.state)),
                },
                priority: None,
                assignee: issue.assignee.as_ref().map(|a| a.login.clone()),
                updated_at: issue.updated_at.to_rfc3339(),
                url: issue.html_url.to_string(),
                labels: issue.labels.iter().map(|l| l.name.clone()).collect(),
                team: None,
                project: None,
                milestone: issue.milestone.as_ref().map(|m| m.title.clone()),
                due_date: None,
                created_at: issue.created_at.to_rfc3339(),
                creator: Some(issue.user.login.clone()),
            })
            .collect();

        Ok(issues)
    }
}

impl GitHubProvider {
    fn convert_github_issue(&self, issue: &octocrab::models::issues::Issue) -> RemoteIssue {
        let status = match issue.state {
            octocrab::models::IssueState::Open => RemoteStatus::Open,
            octocrab::models::IssueState::Closed => RemoteStatus::Closed,
            _ => RemoteStatus::Custom(format!("{:?}", issue.state)),
        };

        let labels: Vec<String> = issue.labels.iter().map(|l| l.name.clone()).collect();

        RemoteIssue {
            id: issue.number.to_string(),
            title: issue.title.clone(),
            body: issue.body.clone().unwrap_or_default(),
            status,
            priority: None,
            assignee: issue.assignee.as_ref().map(|a| a.login.clone()),
            updated_at: issue.updated_at.to_rfc3339(),
            url: issue.html_url.to_string(),
            labels,
            team: None,
            project: None,
            milestone: issue.milestone.as_ref().map(|m| m.title.clone()),
            due_date: None,
            created_at: issue.created_at.to_rfc3339(),
            creator: Some(issue.user.login.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_github_provider_new() {
        let provider = GitHubProvider::new("test_token");
        assert!(provider.is_ok());
    }

    #[tokio::test]
    async fn test_github_provider_new_empty_token() {
        let provider = GitHubProvider::new("");
        assert!(provider.is_ok());
    }

    #[tokio::test]
    async fn test_github_provider_with_defaults() {
        let provider = GitHubProvider::new("test_token")
            .unwrap()
            .with_defaults("owner".to_string(), "repo".to_string());

        assert_eq!(provider.default_owner, Some("owner".to_string()));
        assert_eq!(provider.default_repo, Some("repo".to_string()));
    }
}
