//! GitHub Issues provider implementation.

use octocrab::Octocrab;
use secrecy::SecretBox;
use std::fmt;

use crate::error::{JanusError, Result};

use crate::config::Config;

use super::{
    AsHttpError, IssueUpdates, RemoteIssue, RemoteProvider, RemoteQuery, RemoteRef, RemoteStatus,
};

/// GitHub Issues provider
pub struct GitHubProvider {
    client: Octocrab,
    /// GitHub personal access token (stored securely for zeroization on drop)
    #[allow(dead_code)]
    token: SecretBox<String>,
    /// Default owner for creating issues
    default_owner: Option<String>,
    /// Default repo for creating issues
    default_repo: Option<String>,
}

impl fmt::Debug for GitHubProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitHubProvider")
            .field("client", &"<Octocrab>")
            .field("token", &"[REDACTED]")
            .field("default_owner", &self.default_owner)
            .field("default_repo", &self.default_repo)
            .finish()
    }
}

impl GitHubProvider {
    /// Create a new GitHub provider from configuration
    ///
    /// Note: Octocrab 0.48 does not support custom HTTP client timeout configuration.
    /// This uses Octocrab's default HTTP client configuration.
    pub fn from_config(config: &Config) -> Result<Self> {
        let token = config.github_token().ok_or_else(|| {
            JanusError::Auth(
                "GitHub token not configured. Set GITHUB_TOKEN environment variable or run: janus config set github.token <token>".to_string()
            )
        })?;

        let client = Octocrab::builder()
            .personal_token(token.clone())
            .build()
            .map_err(|e| {
                let scrubbed = scrub_token_from_error(&e.to_string(), &token);
                JanusError::Api(format!("Failed to create GitHub client: {scrubbed}"))
            })?;

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
            token: SecretBox::new(Box::new(token)),
            default_owner,
            default_repo,
        })
    }

    /// Create a new GitHub provider with a token
    ///
    /// Note: Octocrab 0.48 does not support custom HTTP client timeout configuration.
    /// This uses Octocrab's default HTTP client configuration.
    pub fn new(token: &str) -> Result<Self> {
        if token.trim().is_empty() {
            return Err(JanusError::Auth("GitHub token cannot be empty".to_string()));
        }

        let token_owned = token.to_string();
        let client = Octocrab::builder()
            .personal_token(token_owned.clone())
            .build()
            .map_err(|e| {
                let scrubbed = scrub_token_from_error(&e.to_string(), &token_owned);
                JanusError::Api(format!("Failed to create GitHub client: {scrubbed}"))
            })?;

        Ok(Self {
            client,
            token: SecretBox::new(Box::new(token_owned)),
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

impl GitHubProvider {}

/// Scrub token patterns from error messages to prevent credential leakage.
///
/// This function removes potential token values from error messages as a defense-in-depth
/// measure, even though Octocrab should not include tokens in error messages.
fn scrub_token_from_error(error_msg: &str, token: &str) -> String {
    // Common token patterns to scrub
    let patterns = [
        token.to_string(),
        format!("Bearer {token}"),
        format!("token {token}"),
        format!("Authorization: {token}"),
        format!("Authorization: Bearer {token}"),
    ];

    let mut result = error_msg.to_string();
    for pattern in &patterns {
        result = result.replace(pattern, "[REDACTED]");
    }

    // Apply regex-based scrubbing for all GitHub token patterns
    scrub_github_tokens(&result)
}

/// Scrub common GitHub token patterns from a string using regex.
///
/// This provides defense-in-depth by removing any GitHub token patterns
/// even when the specific token value is not known.
fn scrub_github_tokens(input: &str) -> String {
    // Regex for common GitHub token formats:
    // - ghp_*, gho_*, ghu_*, ghs_*, ghr_* (classic tokens and OAuth)
    // - github_pat_* (fine-grained personal access tokens)
    // - v1.* or v2.* followed by hex (newer token formats)
    static TOKEN_REGEX: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
        regex::Regex::new(
            r"(gh[pousr]_[A-Za-z0-9_]{36,}|github_pat_[A-Za-z0-9_]{22,}_[A-Za-z0-9_]*|v\d+\.[A-Fa-f0-9]{40,})"
        ).expect("Valid regex pattern")
    });

    TOKEN_REGEX.replace_all(input, "[REDACTED]").to_string()
}

/// Wrapper for GitHub API errors that implements AsHttpError
pub struct GitHubError {
    inner: octocrab::Error,
}

impl fmt::Display for GitHubError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl From<octocrab::Error> for GitHubError {
    fn from(error: octocrab::Error) -> Self {
        Self { inner: error }
    }
}

impl AsHttpError for GitHubError {
    fn as_http_error(&self) -> Option<(reqwest::StatusCode, Option<u64>)> {
        if let octocrab::Error::GitHub { source, .. } = &self.inner {
            let status = reqwest::StatusCode::from_u16(source.status_code.as_u16()).ok()?;
            let is_rate_limited = status.as_u16() == 403 || status.as_u16() == 429;
            return Some((status, is_rate_limited.then_some(60)));
        }
        None
    }

    fn is_transient(&self) -> bool {
        use super::error;
        error::is_github_transient(&self.inner)
    }

    fn is_rate_limited(&self) -> bool {
        use super::error;
        error::is_github_rate_limited(&self.inner)
    }

    fn get_retry_after(&self) -> Option<std::time::Duration> {
        use super::error;
        error::get_github_retry_after(&self.inner)
    }
}

impl From<GitHubError> for JanusError {
    fn from(error: GitHubError) -> Self {
        use super::error;

        if error.is_rate_limited() {
            return if let Some(duration) = error.get_retry_after() {
                JanusError::RateLimited(duration.as_secs())
            } else {
                JanusError::RateLimited(60)
            };
        }

        let message = error::build_github_error_message(&error.inner);
        // Scrub any potential token patterns from the error message
        let scrubbed = scrub_github_tokens(&message);
        JanusError::Api(scrubbed)
    }
}

impl RemoteProvider for GitHubProvider {
    fn fetch_issue<'a>(
        &'a self,
        remote_ref: &'a RemoteRef,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<RemoteIssue>> + Send + 'a>> {
        Box::pin(async move {
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
            let issue = super::execute_with_retry(|| async {
                client
                    .issues(owner, repo)
                    .get(issue_number)
                    .await
                    .map_err(GitHubError::from)
            })
            .await
            .map_err(|e| {
                if let JanusError::Api(msg) = &e
                    && msg.contains("404")
                {
                    JanusError::RemoteIssueNotFound(remote_ref.to_string())
                } else {
                    e
                }
            })?;

            Ok(self.convert_github_issue(&issue))
        })
    }

    fn create_issue<'a>(
        &'a self,
        title: &str,
        body: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<RemoteRef>> + Send + 'a>> {
        let title = title.to_string();
        let body = body.to_string();
        Box::pin(async move {
            let (owner, repo) = self.get_default_owner_repo()?;
            let owner = owner.to_string();
            let repo = repo.to_string();

            let client = self.client.clone();
            let issue = super::execute_with_retry(|| async {
                client
                    .issues(&owner, &repo)
                    .create(&title)
                    .body(&body)
                    .send()
                    .await
                    .map_err(GitHubError::from)
            })
            .await?;

            Ok(RemoteRef::GitHub {
                owner,
                repo,
                issue_number: issue.number,
            })
        })
    }

    fn update_issue<'a>(
        &'a self,
        remote_ref: &'a RemoteRef,
        updates: IssueUpdates,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
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
                RemoteStatus::Closed | RemoteStatus::Custom(_) => {
                    octocrab::models::IssueState::Closed
                }
            });

            if title.is_none() && body.is_none() && state.is_none() {
                return Ok(());
            }

            let client = self.client.clone();
            let owner = owner.to_string();
            let repo = repo.to_string();

            let _ = super::execute_with_retry(|| async {
                let issues = client.issues(&owner, &repo);
                let mut builder = issues.update(issue_number);
                if let Some(t) = &title {
                    builder = builder.title(t);
                }
                if let Some(b) = &body {
                    builder = builder.body(b);
                }
                if let Some(s) = &state {
                    builder = builder.state(s.clone());
                }
                builder.send().await.map_err(GitHubError::from)
            })
            .await?;

            Ok(())
        })
    }

    fn list_issues<'a>(
        &'a self,
        query: &'a RemoteQuery,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<RemoteIssue>>> + Send + 'a>>
    {
        Box::pin(async move {
            let (owner, repo) = self.get_default_owner_repo()?;
            let client = self.client.clone();
            let owner = owner.to_string();
            let repo = repo.to_string();

            let result = super::execute_with_retry(|| async {
                client
                    .issues(&owner, &repo)
                    .list()
                    .per_page(query.limit.min(100) as u8)
                    .send()
                    .await
                    .map_err(GitHubError::from)
            })
            .await?;

            let issues: Vec<RemoteIssue> = result
                .items
                .into_iter()
                .filter(|issue| issue.pull_request.is_none())
                .map(|issue| self.convert_github_issue(&issue))
                .collect();

            Ok(issues)
        })
    }

    fn search_issues<'a>(
        &'a self,
        text: &str,
        limit: u32,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<RemoteIssue>>> + Send + 'a>>
    {
        let text = text.to_string();
        Box::pin(async move {
            let (owner, repo) = self.get_default_owner_repo()?;
            let client = self.client.clone();
            let owner = owner.to_string();
            let repo = repo.to_string();
            let query_str = format!("repo:{owner}/{repo} is:issue {text}");

            let result = super::execute_with_retry(|| async {
                client
                    .search()
                    .issues_and_pull_requests(&query_str)
                    .per_page(limit.min(100) as u8)
                    .send()
                    .await
                    .map_err(GitHubError::from)
            })
            .await?;

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
        })
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
