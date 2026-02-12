//! GitHub Issues provider implementation.

use http::Uri;
use octocrab::Octocrab;
use secrecy::SecretBox;
use std::fmt;

use crate::error::{JanusError, Result};

use crate::config::Config;

use super::{
    AsHttpError, IssueUpdates, PaginatedResult, RemoteIssue, RemoteProvider, RemoteQuery,
    RemoteRef, RemoteStatus,
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
    /// Timeout for remote operations
    timeout: std::time::Duration,
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
            timeout: config.remote_timeout(),
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
            timeout: std::time::Duration::from_secs(30),
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

/// Wrapper that distinguishes HTTP 404 (not-found) from other GitHub errors.
///
/// Used by `fetch_issue` to detect not-found via structured status codes
/// before errors are converted into generic `JanusError` strings.
enum NotFoundOrOther {
    NotFound(GitHubError),
    Other(GitHubError),
}

impl fmt::Display for NotFoundOrOther {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotFoundOrOther::NotFound(e) | NotFoundOrOther::Other(e) => write!(f, "{e}"),
        }
    }
}

impl AsHttpError for NotFoundOrOther {
    fn as_http_error(&self) -> Option<(reqwest::StatusCode, Option<u64>)> {
        match self {
            NotFoundOrOther::NotFound(e) | NotFoundOrOther::Other(e) => e.as_http_error(),
        }
    }

    fn is_transient(&self) -> bool {
        match self {
            NotFoundOrOther::NotFound(_) => false,
            NotFoundOrOther::Other(e) => e.is_transient(),
        }
    }

    fn is_rate_limited(&self) -> bool {
        match self {
            NotFoundOrOther::NotFound(_) => false,
            NotFoundOrOther::Other(e) => e.is_rate_limited(),
        }
    }

    fn get_retry_after(&self) -> Option<std::time::Duration> {
        match self {
            NotFoundOrOther::NotFound(_) => None,
            NotFoundOrOther::Other(e) => e.get_retry_after(),
        }
    }
}

impl From<NotFoundOrOther> for JanusError {
    fn from(error: NotFoundOrOther) -> Self {
        match error {
            NotFoundOrOther::NotFound(e) => {
                // Extract the remote_ref info from the error context is not possible here,
                // so we produce a generic RemoteIssueNotFound. The caller in fetch_issue
                // will see this variant and keep it as-is.
                //
                // We still need a placeholder - the actual ref string is set by the
                // map_err in fetch_issue. Use the underlying error message as context.
                let message = super::error::build_github_error_message(&e.inner);
                let scrubbed = scrub_github_tokens(&message);
                JanusError::RemoteIssueNotFound(scrubbed)
            }
            NotFoundOrOther::Other(e) => JanusError::from(e),
        }
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
            let timeout = self.timeout;
            let remote_ref_str = remote_ref.to_string();
            let issue = super::execute_with_retry(
                || async {
                    client
                        .issues(owner, repo)
                        .get(issue_number)
                        .await
                        .map_err(|e| {
                            let gh_err = GitHubError::from(e);
                            // Check for 404 via structured status code before
                            // the error is converted to a generic JanusError
                            if let Some((status, _)) = gh_err.as_http_error() {
                                if status == reqwest::StatusCode::NOT_FOUND {
                                    return NotFoundOrOther::NotFound(gh_err);
                                }
                            }
                            NotFoundOrOther::Other(gh_err)
                        })
                },
                Some(timeout),
            )
            .await
            .map_err(|e| match e {
                // Structured 404 detected via status code — use the proper ref
                JanusError::RemoteIssueNotFound(_) => {
                    JanusError::RemoteIssueNotFound(remote_ref_str.clone())
                }
                // Fallback: check error message for "404" in case the
                // structured status code was unavailable (e.g. non-GitHub
                // error variant from octocrab)
                JanusError::Api(ref msg) if msg.contains("404") => {
                    JanusError::RemoteIssueNotFound(remote_ref_str.clone())
                }
                other => other,
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
            let timeout = self.timeout;
            let issue = super::execute_with_retry(
                || async {
                    client
                        .issues(&owner, &repo)
                        .create(&title)
                        .body(&body)
                        .send()
                        .await
                        .map_err(GitHubError::from)
                },
                Some(timeout),
            )
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
            let timeout = self.timeout;

            let _ = super::execute_with_retry(
                || async {
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
                },
                Some(timeout),
            )
            .await?;

            Ok(())
        })
    }

    fn browse_issues<'a>(
        &'a self,
        query: &'a RemoteQuery,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<PaginatedResult<RemoteIssue>>> + Send + 'a>,
    > {
        Box::pin(async move {
            let (owner, repo) = self.get_default_owner_repo()?;
            let client = self.client.clone();
            let owner = owner.to_string();
            let repo = repo.to_string();
            let timeout = self.timeout;
            let max_pages = query.max_pages.max(1); // At least 1 page
            let per_page = query.limit.min(100) as u8;

            let result = super::execute_with_retry(
                || async {
                    client
                        .issues(&owner, &repo)
                        .list()
                        .per_page(per_page)
                        .send()
                        .await
                        .map_err(GitHubError::from)
                },
                Some(timeout),
            )
            .await;

            match result {
                Ok(first_page) => {
                    let mut all_issues: Vec<RemoteIssue> = first_page
                        .items
                        .iter()
                        .map(|issue| self.convert_github_issue(issue))
                        .collect();

                    let mut current_page = first_page;
                    let mut pages_fetched = 1u32;
                    let mut has_more = false;
                    let mut next_page_num: Option<u32> = None;

                    // Fetch additional pages up to max_pages
                    while pages_fetched < max_pages {
                        match client.get_page(&current_page.next).await {
                            Ok(Some(page)) => {
                                pages_fetched += 1;
                                all_issues.extend(
                                    page.items.iter().map(|i| self.convert_github_issue(i)),
                                );

                                // Check if there are more pages
                                has_more = page.next.is_some();
                                if has_more {
                                    next_page_num = Some(pages_fetched + 1);
                                }

                                current_page = page;
                            }
                            Ok(None) => {
                                has_more = false;
                                break;
                            }
                            Err(e) => {
                                // Partial failure - return what we have with warning
                                eprintln!(
                                    "Warning: Failed to fetch page {}: {}",
                                    pages_fetched + 1,
                                    e
                                );
                                break;
                            }
                        }
                    }

                    // Check if there are more pages beyond what we fetched
                    if pages_fetched >= max_pages && current_page.next.is_some() {
                        has_more = true;
                        next_page_num = Some(max_pages + 1);
                    }

                    Ok(PaginatedResult {
                        items: all_issues,
                        total_count: None, // GitHub list API doesn't provide total
                        has_more,
                        next_cursor: None, // GitHub uses page numbers
                        next_page: next_page_num,
                    })
                }
                Err(e) => Err(e),
            }
        })
    }

    fn search_remote<'a>(
        &'a self,
        text: &str,
        query: &'a RemoteQuery,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<PaginatedResult<RemoteIssue>>> + Send + 'a>,
    > {
        let text = text.to_string();
        Box::pin(async move {
            let (owner, repo) = self.get_default_owner_repo()?;
            let client = self.client.clone();
            let owner = owner.to_string();
            let repo = repo.to_string();
            let query_str = format!("repo:{owner}/{repo} is:issue {text}");
            let timeout = self.timeout;
            let per_page = query.limit.min(100) as u8;

            let result = super::execute_with_retry(
                || async {
                    client
                        .search()
                        .issues_and_pull_requests(&query_str)
                        .per_page(per_page)
                        .send()
                        .await
                        .map_err(GitHubError::from)
                },
                Some(timeout),
            )
            .await;

            match result {
                Ok(first_page) => {
                    let total_count = Some(first_page.total_count.unwrap_or(0));
                    // Save the next page URI before consuming items
                    let mut next_page_uri: Option<Uri> = first_page.next.clone();

                    let mut all_issues: Vec<RemoteIssue> = first_page
                        .items
                        .into_iter()
                        .filter(|item| item.pull_request.is_none()) // Exclude PRs
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

                    let mut has_more = next_page_uri.is_some();

                    // For search, we fetch all pages (no max limit)
                    // but be careful not to loop forever
                    loop {
                        if next_page_uri.is_none() {
                            break;
                        }

                        match client
                            .get_page::<octocrab::models::issues::Issue>(&Some(
                                next_page_uri.clone().unwrap(),
                            ))
                            .await
                        {
                            Ok(Some(page)) => {
                                all_issues.extend(
                                    page.items
                                        .into_iter()
                                        .filter(|item: &octocrab::models::issues::Issue| {
                                            item.pull_request.is_none()
                                        })
                                        .map(|issue| RemoteIssue {
                                            id: issue.number.to_string(),
                                            title: issue.title.clone(),
                                            body: issue.body.clone().unwrap_or_default(),
                                            status: match issue.state {
                                                octocrab::models::IssueState::Open => {
                                                    RemoteStatus::Open
                                                }
                                                octocrab::models::IssueState::Closed => {
                                                    RemoteStatus::Closed
                                                }
                                                _ => RemoteStatus::Custom(format!(
                                                    "{:?}",
                                                    issue.state
                                                )),
                                            },
                                            priority: None,
                                            assignee: issue
                                                .assignee
                                                .as_ref()
                                                .map(|a| a.login.clone()),
                                            updated_at: issue.updated_at.to_rfc3339(),
                                            url: issue.html_url.to_string(),
                                            labels: issue
                                                .labels
                                                .iter()
                                                .map(|l| l.name.clone())
                                                .collect(),
                                            team: None,
                                            project: None,
                                            milestone: issue
                                                .milestone
                                                .as_ref()
                                                .map(|m| m.title.clone()),
                                            due_date: None,
                                            created_at: issue.created_at.to_rfc3339(),
                                            creator: Some(issue.user.login.clone()),
                                        }),
                                );

                                has_more = page.next.is_some();
                                next_page_uri = page.next;

                                if !has_more {
                                    break;
                                }
                            }
                            Ok(None) => {
                                has_more = false;
                                break;
                            }
                            Err(e) => {
                                // Partial failure in search - return what we have
                                eprintln!("Warning: Search pagination failed: {e}");
                                has_more = true; // Assume there might be more
                                break;
                            }
                        }
                    }

                    Ok(PaginatedResult {
                        items: all_issues,
                        total_count,
                        has_more,
                        next_cursor: None,
                        next_page: None,
                    })
                }
                Err(e) => Err(e),
            }
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
