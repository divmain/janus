//! Linear.app provider implementation using GraphQL API with type-safe cynic queries.

use reqwest::Client;
use std::time::Duration;

use crate::error::{JanusError, Result};

use super::{
    Config, IssueUpdates, Platform, RemoteIssue, RemoteProvider, RemoteQuery, RemoteRef,
    RemoteStatus,
};

const LINEAR_API_URL: &str = "https://api.linear.app/graphql";
const MAX_RETRIES: u32 = 3;

mod graphql {
    // Re-export cynic types we need
    pub use cynic::{GraphQlResponse, MutationBuilder, QueryBuilder};

    // Import schema from the dedicated janus-schema crate.
    // The import MUST be named `schema` for cynic derives to work.
    use janus_schema::linear as schema;

    use serde::Deserialize;

    /// Custom error extensions type for Linear API errors
    #[derive(Debug, Clone, Deserialize, PartialEq)]
    #[serde(rename_all = "camelCase")]
    pub struct ErrorExtensions {
        pub code: Option<String>,
        pub typ: Option<String>,
        pub user_error: Option<bool>,
        pub user_presentable_message: Option<String>,
    }

    // Custom Scalars

    /// DateTime scalar from Linear API (ISO 8601 formatted string)
    #[derive(cynic::Scalar, Debug, Clone)]
    #[cynic(graphql_type = "DateTime")]
    pub struct DateTime(pub String);

    // Query Variables

    /// Variables for fetching a single issue by ID
    #[derive(cynic::QueryVariables, Debug)]
    pub struct IssueQueryVariables {
        pub id: String,
    }

    /// Variables for fetching teams
    #[derive(cynic::QueryVariables, Debug)]
    pub struct TeamsQueryVariables {
        pub first: Option<i32>,
    }

    /// Variables for fetching multiple issues
    #[derive(cynic::QueryVariables, Debug)]
    pub struct IssuesQueryVariables {
        pub first: Option<i32>,
        pub after: Option<String>,
        pub filter: Option<IssueFilter>,
    }

    // Filter Input Objects

    /// Issue filtering options for server-side search.
    /// Used to filter issues by title, description, and other fields.
    #[derive(cynic::InputObject, Debug, Clone, Default)]
    #[cynic(rename_all = "camelCase")]
    pub struct IssueFilter {
        /// Comparator for the issue's title
        pub title: Option<StringComparator>,
        /// Comparator for the issue's description
        pub description: Option<NullableStringComparator>,
        /// Compound filters using logical OR (any must match)
        pub or: Option<Vec<IssueFilter>>,
        /// Compound filters using logical AND (all must match)
        pub and: Option<Vec<IssueFilter>>,
    }

    /// Comparator for string fields (non-nullable).
    #[derive(cynic::InputObject, Debug, Clone, Default)]
    #[cynic(rename_all = "camelCase")]
    pub struct StringComparator {
        /// Equals constraint
        pub eq: Option<String>,
        /// Not-equals constraint
        pub neq: Option<String>,
        /// In-array constraint
        #[cynic(rename = "in")]
        pub in_: Option<Vec<String>>,
        /// Not-in-array constraint
        pub nin: Option<Vec<String>>,
        /// Contains constraint (case sensitive)
        pub contains: Option<String>,
        /// Contains constraint (case insensitive)
        pub contains_ignore_case: Option<String>,
        /// Starts with constraint
        pub starts_with: Option<String>,
        /// Ends with constraint
        pub ends_with: Option<String>,
    }

    /// Comparator for optional string fields.
    #[derive(cynic::InputObject, Debug, Clone, Default)]
    #[cynic(rename_all = "camelCase")]
    pub struct NullableStringComparator {
        /// Equals constraint
        pub eq: Option<String>,
        /// Not-equals constraint
        pub neq: Option<String>,
        /// In-array constraint
        #[cynic(rename = "in")]
        pub in_: Option<Vec<String>>,
        /// Not-in-array constraint
        pub nin: Option<Vec<String>>,
        /// Contains constraint (case sensitive)
        pub contains: Option<String>,
        /// Contains constraint (case insensitive)
        pub contains_ignore_case: Option<String>,
        /// Starts with constraint
        pub starts_with: Option<String>,
        /// Ends with constraint
        pub ends_with: Option<String>,
        /// Null constraint - matches null values if true, non-null if false
        pub null: Option<bool>,
    }

    /// Variables for creating an issue
    #[derive(cynic::QueryVariables, Debug)]
    pub struct IssueCreateVariables {
        pub input: IssueCreateInput,
    }

    /// Variables for updating an issue
    #[derive(cynic::QueryVariables, Debug)]
    pub struct IssueUpdateVariables {
        pub id: String,
        pub input: IssueUpdateInput,
    }

    // Input Objects

    /// Input for creating an issue
    #[derive(cynic::InputObject, Debug, Clone)]
    #[cynic(rename_all = "camelCase")]
    pub struct IssueCreateInput {
        /// The title of the issue
        pub title: Option<String>,
        /// The issue description in markdown format
        pub description: Option<String>,
        /// The identifier of the team associated with the issue
        pub team_id: String,
    }

    /// Input for updating an issue
    #[derive(cynic::InputObject, Debug, Clone, Default)]
    #[cynic(rename_all = "camelCase")]
    pub struct IssueUpdateInput {
        /// The issue title
        pub title: Option<String>,
        /// The issue description in markdown format
        pub description: Option<String>,
    }

    // Query Fragments - Issue Query

    /// Query to fetch a single issue by ID
    #[derive(cynic::QueryFragment, Debug)]
    #[cynic(graphql_type = "Query", variables = "IssueQueryVariables")]
    pub struct IssueQuery {
        #[arguments(id: $id)]
        pub issue: Issue,
    }

    /// Query to fetch multiple issues
    #[derive(cynic::QueryFragment, Debug)]
    #[cynic(graphql_type = "Query", variables = "IssuesQueryVariables")]
    pub struct IssuesQuery {
        #[arguments(first: $first, after: $after, filter: $filter)]
        pub issues: IssueConnection,
    }

    /// Connection of issues
    #[derive(cynic::QueryFragment, Debug)]
    pub struct IssueConnection {
        pub nodes: Vec<Issue>,
        #[allow(dead_code)]
        pub page_info: PageInfo,
    }

    /// Pagination info
    #[derive(cynic::QueryFragment, Debug)]
    pub struct PageInfo {
        #[allow(dead_code)]
        pub has_next_page: bool,
        #[allow(dead_code)]
        pub end_cursor: Option<String>,
    }

    /// Issue fragment for fetching issue details
    #[derive(cynic::QueryFragment, Debug)]
    pub struct Issue {
        pub id: cynic::Id,
        pub identifier: String,
        pub title: String,
        pub description: Option<String>,
        pub state: WorkflowState,
        pub priority: f64,
        pub assignee: Option<User>,
        pub updated_at: DateTime,
        pub url: String,
    }

    /// Workflow state of an issue
    #[derive(cynic::QueryFragment, Debug)]
    pub struct WorkflowState {
        pub name: String,
        #[cynic(rename = "type")]
        pub state_type: String,
    }

    /// User information
    #[derive(cynic::QueryFragment, Debug)]
    pub struct User {
        pub name: String,
    }

    // Query Fragments - Teams Query

    /// Query to fetch teams
    #[derive(cynic::QueryFragment, Debug)]
    #[cynic(graphql_type = "Query", variables = "TeamsQueryVariables")]
    pub struct TeamsQuery {
        #[arguments(first: $first)]
        pub teams: TeamConnection,
    }

    /// Connection of teams
    #[derive(cynic::QueryFragment, Debug)]
    pub struct TeamConnection {
        pub nodes: Vec<Team>,
    }

    /// Team information
    #[derive(cynic::QueryFragment, Debug)]
    pub struct Team {
        pub id: cynic::Id,
        #[allow(dead_code)]
        pub key: String,
    }

    // Mutation Fragments - Create Issue

    /// Mutation to create an issue
    #[derive(cynic::QueryFragment, Debug)]
    #[cynic(graphql_type = "Mutation", variables = "IssueCreateVariables")]
    pub struct IssueCreateMutation {
        #[arguments(input: $input)]
        pub issue_create: IssuePayload,
    }

    /// Payload returned from issue mutations
    #[derive(cynic::QueryFragment, Debug)]
    pub struct IssuePayload {
        pub success: bool,
        pub issue: Option<CreatedIssue>,
    }

    /// Issue fragment for created/updated issue (minimal fields)
    #[derive(cynic::QueryFragment, Debug)]
    #[cynic(graphql_type = "Issue")]
    pub struct CreatedIssue {
        pub identifier: String,
        #[allow(dead_code)]
        pub url: String,
    }

    // Mutation Fragments - Update Issue

    /// Mutation to update an issue
    #[derive(cynic::QueryFragment, Debug)]
    #[cynic(graphql_type = "Mutation", variables = "IssueUpdateVariables")]
    pub struct IssueUpdateMutation {
        #[arguments(id: $id, input: $input)]
        pub issue_update: IssueUpdatePayload,
    }

    /// Payload returned from issue update mutation
    #[derive(cynic::QueryFragment, Debug)]
    #[cynic(graphql_type = "IssuePayload")]
    pub struct IssueUpdatePayload {
        pub success: bool,
    }
}

// Linear Provider Implementation

use graphql::*;

/// Linear.app provider
pub struct LinearProvider {
    client: Client,
    api_key: String,
    /// Default organization for creating issues
    default_org: Option<String>,
    /// Default team ID for creating issues (fetched on first use)
    default_team_id: Option<String>,
}

impl LinearProvider {
    /// Create a new Linear provider from configuration
    pub fn from_config(config: &Config) -> Result<Self> {
        let api_key = config.linear_api_key().ok_or_else(|| {
            JanusError::Auth(
                "Linear API key not configured. Set LINEAR_API_KEY environment variable or run: janus config set linear.api_key <key>".to_string()
            )
        })?;

        let default_org = config.default_remote.as_ref().and_then(|d| {
            if d.platform == Platform::Linear {
                Some(d.org.clone())
            } else {
                None
            }
        });

        Ok(Self {
            client: Client::new(),
            api_key,
            default_org,
            default_team_id: None,
        })
    }

    /// Create a new Linear provider with an API key
    pub fn new(api_key: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
            default_org: None,
            default_team_id: None,
        }
    }

    /// Set default organization for creating issues
    pub fn with_default_org(mut self, org: String) -> Self {
        self.default_org = Some(org);
        self
    }

    /// Set default team ID for creating issues
    pub fn with_default_team_id(mut self, team_id: String) -> Self {
        self.default_team_id = Some(team_id);
        self
    }

    /// Check if an HTTP status is a transient error (5xx) that should be retried
    fn is_transient_error(status: reqwest::StatusCode) -> bool {
        status.is_server_error() // 5xx
    }

    /// Execute a GraphQL operation (query or mutation) with retry logic
    async fn execute<ResponseData, Vars>(
        &self,
        operation: cynic::Operation<ResponseData, Vars>,
    ) -> Result<ResponseData>
    where
        ResponseData: serde::de::DeserializeOwned + 'static,
        Vars: serde::Serialize,
    {
        let mut retries = 0;

        loop {
            let response = self
                .client
                .post(LINEAR_API_URL)
                .header("Authorization", &self.api_key)
                .header("Content-Type", "application/json")
                .json(&operation)
                .send()
                .await?;

            let status = response.status();

            // Check for rate limit (HTTP 429) - use longer wait with Retry-After header
            if status.as_u16() == 429 && retries < MAX_RETRIES {
                retries += 1;

                let retry_after = response
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok());

                let wait_duration = match retry_after {
                    Some(seconds) => Duration::from_secs(seconds),
                    None => Duration::from_secs(60),
                };

                tokio::time::sleep(wait_duration).await;
                continue;
            }

            // Check for transient errors (5xx) - use exponential backoff
            if Self::is_transient_error(status) && retries < MAX_RETRIES {
                let base_delay_ms = 100u64;
                let delay_ms = base_delay_ms * 2u64.pow(retries);
                retries += 1;
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                continue;
            }

            if !status.is_success() {
                let text = response.text().await.unwrap_or_default();
                return Err(JanusError::Api(format!(
                    "Linear API error ({}): {}",
                    status, text
                )));
            }

            let result: GraphQlResponse<ResponseData, ErrorExtensions> = response.json().await?;

            if let Some(errors) = result.errors {
                let error_msgs: Vec<String> = errors
                    .iter()
                    .map(|e| {
                        if let Some(ext) = &e.extensions
                            && let Some(code) = &ext.code
                        {
                            return format!("[{}] {}", code, e.message);
                        }
                        e.message.clone()
                    })
                    .collect();
                return Err(JanusError::Api(format!(
                    "Linear GraphQL errors: {}",
                    error_msgs.join(", ")
                )));
            }

            return result
                .data
                .ok_or_else(|| JanusError::Api("No data in Linear response".to_string()));
        }
    }

    /// Fetch the first team ID for this organization
    async fn fetch_default_team_id(&self) -> Result<String> {
        let operation = TeamsQuery::build(TeamsQueryVariables { first: Some(1) });

        let response = self.execute(operation).await?;

        response
            .teams
            .nodes
            .into_iter()
            .next()
            .map(|t| t.id.into_inner())
            .ok_or_else(|| JanusError::Api("No teams found in Linear workspace".to_string()))
    }

    /// Check if an error is a NOT_FOUND error from Linear API
    fn is_not_found_error(error: &JanusError) -> bool {
        if let JanusError::Api(msg) = error {
            msg.contains("[NOT_FOUND]")
        } else {
            false
        }
    }
}

impl RemoteProvider for LinearProvider {
    async fn fetch_issue(&self, remote_ref: &RemoteRef) -> Result<RemoteIssue> {
        let issue_id = match remote_ref {
            RemoteRef::Linear { issue_id, .. } => issue_id,
            _ => {
                return Err(JanusError::Api(
                    "LinearProvider can only fetch Linear issues".to_string(),
                ));
            }
        };

        let operation = IssueQuery::build(IssueQueryVariables {
            id: issue_id.clone(),
        });

        let response = self.execute(operation).await.map_err(|e| {
            if Self::is_not_found_error(&e) {
                JanusError::RemoteIssueNotFound(remote_ref.to_string())
            } else {
                e
            }
        })?;

        Ok(self.convert_linear_issue(response.issue))
    }

    async fn create_issue(&self, title: &str, body: &str) -> Result<RemoteRef> {
        let team_id = match &self.default_team_id {
            Some(id) => id.clone(),
            None => self.fetch_default_team_id().await?,
        };

        let operation = IssueCreateMutation::build(IssueCreateVariables {
            input: IssueCreateInput {
                title: Some(title.to_string()),
                description: Some(body.to_string()),
                team_id,
            },
        });

        let response = self.execute(operation).await?;

        if !response.issue_create.success {
            return Err(JanusError::Api("Failed to create Linear issue".to_string()));
        }

        let issue = response
            .issue_create
            .issue
            .ok_or_else(|| JanusError::Api("No issue returned from Linear".to_string()))?;

        let org = self.default_org.as_ref().ok_or_else(|| {
            JanusError::Config("No default Linear organization configured".to_string())
        })?;

        Ok(RemoteRef::Linear {
            org: org.clone(),
            issue_id: issue.identifier,
        })
    }

    async fn update_issue(&self, remote_ref: &RemoteRef, updates: IssueUpdates) -> Result<()> {
        let issue_id = match remote_ref {
            RemoteRef::Linear { issue_id, .. } => issue_id,
            _ => {
                return Err(JanusError::Api(
                    "LinearProvider can only update Linear issues".to_string(),
                ));
            }
        };

        // First, get the internal UUID for this issue by fetching it
        let fetch_operation = IssueQuery::build(IssueQueryVariables {
            id: issue_id.clone(),
        });

        let fetch_response = self.execute(fetch_operation).await.map_err(|e| {
            if Self::is_not_found_error(&e) {
                JanusError::RemoteIssueNotFound(remote_ref.to_string())
            } else {
                e
            }
        })?;

        let internal_id = fetch_response.issue.id.into_inner();

        // Build update input
        let input = IssueUpdateInput {
            title: updates.title,
            description: updates.body,
        };

        // Check if there's anything to update
        if input.title.is_none() && input.description.is_none() {
            return Ok(());
        }

        let operation = IssueUpdateMutation::build(IssueUpdateVariables {
            id: internal_id,
            input,
        });

        let response = self.execute(operation).await?;

        if !response.issue_update.success {
            return Err(JanusError::Api("Failed to update Linear issue".to_string()));
        }

        Ok(())
    }

    async fn list_issues(
        &self,
        query: &RemoteQuery,
    ) -> std::result::Result<Vec<RemoteIssue>, crate::error::JanusError> {
        let operation = IssuesQuery::build(IssuesQueryVariables {
            first: Some(query.limit as i32),
            after: query.cursor.clone(),
            filter: None,
        });

        let response = self.execute(operation).await?;

        let issues: Vec<RemoteIssue> = response
            .issues
            .nodes
            .into_iter()
            .map(|issue| self.convert_linear_issue(issue))
            .collect();

        Ok(issues)
    }

    async fn search_issues(
        &self,
        text: &str,
        limit: u32,
    ) -> std::result::Result<Vec<RemoteIssue>, crate::error::JanusError> {
        // Use Linear's server-side filtering with IssueFilter.
        // We search title, description, and identifier using case-insensitive contains.
        // The filter uses OR logic: match if any of the fields contain the search text.

        let title_filter = IssueFilter {
            title: Some(StringComparator {
                contains_ignore_case: Some(text.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let description_filter = IssueFilter {
            description: Some(NullableStringComparator {
                contains_ignore_case: Some(text.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Combine filters with OR logic: match title OR description
        let filter = IssueFilter {
            or: Some(vec![title_filter, description_filter]),
            ..Default::default()
        };

        let operation = IssuesQuery::build(IssuesQueryVariables {
            first: Some(limit as i32),
            after: None,
            filter: Some(filter),
        });

        let response = self.execute(operation).await?;

        let issues: Vec<RemoteIssue> = response
            .issues
            .nodes
            .into_iter()
            .map(|issue| self.convert_linear_issue(issue))
            .collect();

        Ok(issues)
    }
}

impl LinearProvider {
    fn convert_linear_issue(&self, issue: Issue) -> RemoteIssue {
        let status = match issue.state.state_type.as_str() {
            "completed" => RemoteStatus::Closed,
            "canceled" => RemoteStatus::Custom("Cancelled".to_string()),
            _ => RemoteStatus::Custom(issue.state.name.clone()),
        };

        let priority = {
            let p = issue.priority as i32;
            Some(match p {
                0 => 2,
                1 => 0,
                2 => 1,
                3 => 2,
                4 => 3,
                _ => 4,
            } as u8)
        };

        RemoteIssue {
            id: issue.identifier,
            title: issue.title,
            body: issue.description.unwrap_or_default(),
            status,
            priority,
            assignee: issue.assignee.map(|a| a.name),
            updated_at: issue.updated_at.0,
            url: issue.url,
            labels: vec![],
            team: None,
            project: None,
            milestone: None,
            due_date: None,
            created_at: jiff::Timestamp::now().to_string(),
            creator: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_conversion() {
        let provider = LinearProvider::new("test_api_key");

        let test_issue = Issue {
            id: cynic::Id::new("test-id"),
            identifier: "ENG-123".to_string(),
            title: "Test Issue".to_string(),
            description: Some("Description".to_string()),
            state: WorkflowState {
                name: "In Progress".to_string(),
                state_type: "started".to_string(),
            },
            priority: 2.0,
            assignee: Some(User {
                name: "Test User".to_string(),
            }),
            updated_at: DateTime("2024-01-01T00:00:00Z".to_string()),
            url: "https://linear.app/issue/ENG-123".to_string(),
        };

        let converted = provider.convert_linear_issue(test_issue);
        assert_eq!(converted.priority, Some(1));

        let test_issue_p0 = Issue {
            id: cynic::Id::new("test-id"),
            identifier: "ENG-123".to_string(),
            title: "Test Issue".to_string(),
            description: Some("Description".to_string()),
            state: WorkflowState {
                name: "In Progress".to_string(),
                state_type: "started".to_string(),
            },
            priority: 1.0,
            assignee: Some(User {
                name: "Test User".to_string(),
            }),
            updated_at: DateTime("2024-01-01T00:00:00Z".to_string()),
            url: "https://linear.app/issue/ENG-123".to_string(),
        };
        let converted_p0 = provider.convert_linear_issue(test_issue_p0);
        assert_eq!(converted_p0.priority, Some(0));

        let test_issue_p4 = Issue {
            id: cynic::Id::new("test-id"),
            identifier: "ENG-123".to_string(),
            title: "Test Issue".to_string(),
            description: Some("Description".to_string()),
            state: WorkflowState {
                name: "In Progress".to_string(),
                state_type: "started".to_string(),
            },
            priority: 4.0,
            assignee: Some(User {
                name: "Test User".to_string(),
            }),
            updated_at: DateTime("2024-01-01T00:00:00Z".to_string()),
            url: "https://linear.app/issue/ENG-123".to_string(),
        };
        let converted_p4 = provider.convert_linear_issue(test_issue_p4);
        assert_eq!(converted_p4.priority, Some(3));
    }

    #[test]
    fn test_priority_out_of_range() {
        let provider = LinearProvider::new("test_api_key");

        let test_issue_p_negative = Issue {
            id: cynic::Id::new("test-id"),
            identifier: "ENG-123".to_string(),
            title: "Test Issue".to_string(),
            description: Some("Description".to_string()),
            state: WorkflowState {
                name: "In Progress".to_string(),
                state_type: "started".to_string(),
            },
            priority: -1.0,
            assignee: Some(User {
                name: "Test User".to_string(),
            }),
            updated_at: DateTime("2024-01-01T00:00:00Z".to_string()),
            url: "https://linear.app/issue/ENG-123".to_string(),
        };
        let converted = provider.convert_linear_issue(test_issue_p_negative);
        assert_eq!(converted.priority, Some(4));
    }

    #[test]
    fn test_status_conversion() {
        let provider = LinearProvider::new("test_api_key");

        let completed_issue = Issue {
            id: cynic::Id::new("test-id"),
            identifier: "ENG-123".to_string(),
            title: "Test Issue".to_string(),
            description: Some("Description".to_string()),
            state: WorkflowState {
                name: "Done".to_string(),
                state_type: "completed".to_string(),
            },
            priority: 2.0,
            assignee: None,
            updated_at: DateTime("2024-01-01T00:00:00Z".to_string()),
            url: "https://linear.app/issue/ENG-123".to_string(),
        };

        assert_eq!(
            provider.convert_linear_issue(completed_issue).status,
            RemoteStatus::Closed
        );

        let canceled_issue = Issue {
            id: cynic::Id::new("test-id"),
            identifier: "ENG-123".to_string(),
            title: "Test Issue".to_string(),
            description: Some("Description".to_string()),
            state: WorkflowState {
                name: "Canceled".to_string(),
                state_type: "canceled".to_string(),
            },
            priority: 2.0,
            assignee: None,
            updated_at: DateTime("2024-01-01T00:00:00Z".to_string()),
            url: "https://linear.app/issue/ENG-123".to_string(),
        };

        assert_eq!(
            provider.convert_linear_issue(canceled_issue).status,
            RemoteStatus::Custom("Cancelled".to_string())
        );

        let custom_issue = Issue {
            id: cynic::Id::new("test-id"),
            identifier: "ENG-123".to_string(),
            title: "Test Issue".to_string(),
            description: Some("Description".to_string()),
            state: WorkflowState {
                name: "Backlog".to_string(),
                state_type: "backlog".to_string(),
            },
            priority: 2.0,
            assignee: None,
            updated_at: DateTime("2024-01-01T00:00:00Z".to_string()),
            url: "https://linear.app/issue/ENG-123".to_string(),
        };

        assert_eq!(
            provider.convert_linear_issue(custom_issue).status,
            RemoteStatus::Custom("Backlog".to_string())
        );
    }

    #[test]
    fn test_issue_without_description() {
        let provider = LinearProvider::new("test_api_key");

        let test_issue = Issue {
            id: cynic::Id::new("test-id"),
            identifier: "ENG-123".to_string(),
            title: "Test Issue".to_string(),
            description: None,
            state: WorkflowState {
                name: "In Progress".to_string(),
                state_type: "started".to_string(),
            },
            priority: 2.0,
            assignee: None,
            updated_at: DateTime("2024-01-01T00:00:00Z".to_string()),
            url: "https://linear.app/issue/ENG-123".to_string(),
        };

        let converted = provider.convert_linear_issue(test_issue);
        assert_eq!(converted.body, "");
    }

    #[test]
    fn test_issue_without_assignee() {
        let provider = LinearProvider::new("test_api_key");

        let test_issue = Issue {
            id: cynic::Id::new("test-id"),
            identifier: "ENG-123".to_string(),
            title: "Test Issue".to_string(),
            description: Some("Description".to_string()),
            state: WorkflowState {
                name: "In Progress".to_string(),
                state_type: "started".to_string(),
            },
            priority: 2.0,
            assignee: None,
            updated_at: DateTime("2024-01-01T00:00:00Z".to_string()),
            url: "https://linear.app/issue/ENG-123".to_string(),
        };

        let converted = provider.convert_linear_issue(test_issue);
        assert_eq!(converted.assignee, None);
    }

    #[test]
    fn test_issue_fields() {
        let provider = LinearProvider::new("test_api_key");

        let test_issue = Issue {
            id: cynic::Id::new("test-id"),
            identifier: "ENG-123".to_string(),
            title: "Test Issue".to_string(),
            description: Some("Description".to_string()),
            state: WorkflowState {
                name: "In Progress".to_string(),
                state_type: "started".to_string(),
            },
            priority: 2.0,
            assignee: Some(User {
                name: "Test User".to_string(),
            }),
            updated_at: DateTime("2024-01-01T00:00:00Z".to_string()),
            url: "https://linear.app/issue/ENG-123".to_string(),
        };

        let converted = provider.convert_linear_issue(test_issue);

        assert_eq!(converted.id, "ENG-123");
        assert_eq!(converted.title, "Test Issue");
        assert_eq!(converted.body, "Description");
        assert_eq!(converted.url, "https://linear.app/issue/ENG-123");
        assert_eq!(converted.assignee, Some("Test User".to_string()));
        assert_eq!(converted.labels, Vec::<String>::new());
        assert_eq!(converted.team, None);
        assert_eq!(converted.project, None);
        assert_eq!(converted.milestone, None);
        assert_eq!(converted.due_date, None);
        assert_eq!(converted.creator, None);
    }
}
