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
        if token.trim().is_empty() {
            return Err(JanusError::Auth("GitHub token cannot be empty".to_string()));
        }

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

    /// Get default owner and repo, returning an error if not configured
    fn get_default_owner_repo(&self) -> Result<(&str, &str)> {
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

        Ok((owner.as_str(), repo.as_str()))
    }
}

impl GitHubProvider {
    /// Check if an octocrab error is a rate limit error
    fn is_rate_limit_error(error: &octocrab::Error) -> bool {
        let error_msg = error.to_string().to_lowercase();
        error_msg.contains("rate limit")
            || error_msg.contains("api rate limit exceeded")
            || error_msg.contains("403")
    }

    /// Check if an octocrab error is a transient error (5xx, timeout, connection, network)
    fn is_transient_error(error: &octocrab::Error) -> bool {
        let error_msg = error.to_string().to_lowercase();
        if Self::is_rate_limit_error(error) {
            return false;
        }
        error_msg.contains("5")
            || error_msg.contains("timed out")
            || error_msg.contains("timeout")
            || error_msg.contains("connection")
            || error_msg.contains("network")
            || error_msg.contains("service unavailable")
    }

    /// Convert an octocrab error to a JanusError
    fn handle_octocrab_error(error: octocrab::Error) -> JanusError {
        let error_msg = error.to_string();
        let error_msg_lower = error_msg.to_lowercase();

        if error_msg_lower.contains("rate limit")
            || error_msg_lower.contains("api rate limit exceeded")
        {
            return JanusError::RateLimited(60);
        }

        JanusError::Api(format!("GitHub API error: {}", error_msg))
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

        let client = self.client.clone();
        let issue = super::execute_with_retry(
            || async { client.issues(owner, repo).get(issue_number).await },
            Self::is_transient_error,
        )
        .await
        .map_err(|e| {
            let janus_err = Self::handle_octocrab_error(e);
            if let JanusError::Api(msg) = &janus_err
                && msg.contains("404")
            {
                JanusError::RemoteIssueNotFound(remote_ref.to_string())
            } else {
                janus_err
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
        let (owner, repo) = self.get_default_owner_repo()?;
        let owner = owner.to_string();
        let repo = repo.to_string();
        let title = title.to_string();
        let body = body.to_string();

        let client = self.client.clone();
        let issue = super::execute_with_retry(
            || async {
                client
                    .issues(&owner, &repo)
                    .create(&title)
                    .body(&body)
                    .send()
                    .await
            },
            Self::is_transient_error,
        )
        .await
        .map_err(Self::handle_octocrab_error)?;

        Ok(RemoteRef::GitHub {
            owner,
            repo,
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

        let title = updates.title.clone();
        let body = updates.body.clone();
        let status = updates.status.clone();

        let state = status.map(|s| match s {
            RemoteStatus::Open => octocrab::models::IssueState::Open,
            RemoteStatus::Closed | RemoteStatus::Custom(_) => octocrab::models::IssueState::Closed,
        });

        if title.is_none() && body.is_none() && state.is_none() {
            return Ok(());
        }

        let client = self.client.clone();
        let owner = owner.to_string();
        let repo = repo.to_string();

        let _ = super::execute_with_retry(
            || async {
                if let (Some(t), Some(b), Some(s)) = (&title, &body, &state) {
                    client
                        .issues(&owner, &repo)
                        .update(issue_number)
                        .title(t)
                        .body(b)
                        .state(s.clone())
                        .send()
                        .await
                } else if let (Some(t), Some(b), None) = (&title, &body, &state) {
                    client
                        .issues(&owner, &repo)
                        .update(issue_number)
                        .title(t)
                        .body(b)
                        .send()
                        .await
                } else if let (Some(t), None, Some(s)) = (&title, &body, &state) {
                    client
                        .issues(&owner, &repo)
                        .update(issue_number)
                        .title(t)
                        .state(s.clone())
                        .send()
                        .await
                } else if let (None, Some(b), Some(s)) = (&title, &body, &state) {
                    client
                        .issues(&owner, &repo)
                        .update(issue_number)
                        .body(b)
                        .state(s.clone())
                        .send()
                        .await
                } else if let (Some(t), None, None) = (&title, &body, &state) {
                    client
                        .issues(&owner, &repo)
                        .update(issue_number)
                        .title(t)
                        .send()
                        .await
                } else if let (None, Some(b), None) = (&title, &body, &state) {
                    client
                        .issues(&owner, &repo)
                        .update(issue_number)
                        .body(b)
                        .send()
                        .await
                } else if let (None, None, Some(s)) = (&title, &body, &state) {
                    client
                        .issues(&owner, &repo)
                        .update(issue_number)
                        .state(s.clone())
                        .send()
                        .await
                } else {
                    // All None - should have been caught above
                    Err(octocrab::Error::Other {
                        source: Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "No fields to update",
                        )),
                        backtrace: std::backtrace::Backtrace::disabled(),
                    })
                }
            },
            Self::is_transient_error,
        )
        .await
        .map_err(Self::handle_octocrab_error)?;

        Ok(())
    }

    async fn list_issues(
        &self,
        query: &RemoteQuery,
    ) -> std::result::Result<Vec<RemoteIssue>, crate::error::JanusError> {
        let (owner, repo) = self.get_default_owner_repo()?;
        let client = self.client.clone();
        let owner = owner.to_string();
        let repo = repo.to_string();

        let result = super::execute_with_retry(
            || async {
                client
                    .issues(&owner, &repo)
                    .list()
                    .per_page(query.limit as u8)
                    .send()
                    .await
            },
            Self::is_transient_error,
        )
        .await
        .map_err(Self::handle_octocrab_error)?;

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
        let (owner, repo) = self.get_default_owner_repo()?;
        let client = self.client.clone();
        let owner = owner.to_string();
        let repo = repo.to_string();
        let query_str = format!("repo:{}/{} is:issue {}", owner, repo, text);

        let result = super::execute_with_retry(
            || async {
                client
                    .search()
                    .issues_and_pull_requests(&query_str)
                    .per_page(limit.min(100) as u8)
                    .send()
                    .await
            },
            Self::is_transient_error,
        )
        .await
        .map_err(Self::handle_octocrab_error)?;

        let issues: Vec<RemoteIssue> = result
            .items
            .into_iter()
            .filter(|item| item.pull_request.is_none())
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
        assert!(provider.is_err());
    }

    #[tokio::test]
    async fn test_github_provider_new_whitespace_token() {
        let provider = GitHubProvider::new("   ");
        assert!(provider.is_err());
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
