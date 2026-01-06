//! Linear.app provider implementation using GraphQL API.

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{JanusError, Result};

use super::{Config, IssueUpdates, Platform, RemoteIssue, RemoteProvider, RemoteRef, RemoteStatus};

const LINEAR_API_URL: &str = "https://api.linear.app/graphql";

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

    /// Execute a GraphQL query
    async fn graphql<T: for<'de> Deserialize<'de>>(
        &self,
        query: &str,
        variables: Option<serde_json::Value>,
    ) -> Result<T> {
        let body = GraphQLRequest {
            query: query.to_string(),
            variables,
        };

        let response = self
            .client
            .post(LINEAR_API_URL)
            .header("Authorization", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(JanusError::Api(format!(
                "Linear API error ({}): {}",
                status, text
            )));
        }

        let result: GraphQLResponse<T> = response.json().await?;

        if let Some(errors) = result.errors {
            let error_msgs: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
            return Err(JanusError::Api(format!(
                "Linear GraphQL errors: {}",
                error_msgs.join(", ")
            )));
        }

        result
            .data
            .ok_or_else(|| JanusError::Api("No data in Linear response".to_string()))
    }

    /// Fetch the first team ID for this organization
    async fn fetch_default_team_id(&self) -> Result<String> {
        let query = r#"
            query {
                teams(first: 1) {
                    nodes {
                        id
                        key
                    }
                }
            }
        "#;

        #[derive(Deserialize)]
        struct TeamsResponse {
            teams: TeamsConnection,
        }

        #[derive(Deserialize)]
        struct TeamsConnection {
            nodes: Vec<TeamNode>,
        }

        #[derive(Deserialize)]
        struct TeamNode {
            id: String,
        }

        let response: TeamsResponse = self.graphql(query, None).await?;

        response
            .teams
            .nodes
            .into_iter()
            .next()
            .map(|t| t.id)
            .ok_or_else(|| JanusError::Api("No teams found in Linear workspace".to_string()))
    }
}

#[derive(Serialize)]
struct GraphQLRequest {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    variables: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct GraphQLResponse<T> {
    data: Option<T>,
    errors: Option<Vec<GraphQLError>>,
}

#[derive(Deserialize)]
struct GraphQLError {
    message: String,
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

        // Query issue by identifier (e.g., "PROJ-123")
        let query = r#"
            query GetIssue($id: String!) {
                issue(id: $id) {
                    id
                    identifier
                    title
                    description
                    state {
                        name
                        type
                    }
                    priority
                    assignee {
                        name
                    }
                    updatedAt
                    url
                }
            }
        "#;

        let variables = serde_json::json!({
            "id": issue_id
        });

        #[derive(Deserialize)]
        struct IssueResponse {
            issue: Option<LinearIssue>,
        }

        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct LinearIssue {
            id: String,
            identifier: String,
            title: String,
            description: Option<String>,
            state: IssueState,
            priority: Option<i32>,
            assignee: Option<Assignee>,
            #[serde(rename = "updatedAt")]
            updated_at: String,
            url: String,
        }

        #[derive(Deserialize)]
        struct IssueState {
            name: String,
            #[serde(rename = "type")]
            state_type: String,
        }

        #[derive(Deserialize)]
        struct Assignee {
            name: String,
        }

        let response: IssueResponse = self.graphql(query, Some(variables)).await?;

        let issue = response
            .issue
            .ok_or_else(|| JanusError::RemoteIssueNotFound(remote_ref.to_string()))?;

        // Map Linear state type to RemoteStatus
        let status = match issue.state.state_type.as_str() {
            "completed" => RemoteStatus::Closed,
            "canceled" => RemoteStatus::Custom("Cancelled".to_string()),
            _ => RemoteStatus::Custom(issue.state.name.clone()),
        };

        // Linear priority is 0-4, where 0 = no priority, 1 = urgent, 4 = low
        // We map this to our 0-4 scale where 0 = highest
        let priority = issue.priority.map(|p| {
            match p {
                0 => 2, // No priority -> P2 (default)
                1 => 0, // Urgent -> P0
                2 => 1, // High -> P1
                3 => 2, // Medium -> P2
                4 => 3, // Low -> P3
                _ => 4, // Other -> P4
            }
        });

        Ok(RemoteIssue {
            id: issue.identifier,
            title: issue.title,
            body: issue.description.unwrap_or_default(),
            status,
            priority: priority.map(|p| p as u8),
            assignee: issue.assignee.map(|a| a.name),
            updated_at: issue.updated_at,
            url: issue.url,
        })
    }

    async fn create_issue(&self, title: &str, body: &str) -> Result<RemoteRef> {
        let team_id = match &self.default_team_id {
            Some(id) => id.clone(),
            None => self.fetch_default_team_id().await?,
        };

        let query = r#"
            mutation CreateIssue($title: String!, $description: String, $teamId: String!) {
                issueCreate(input: {
                    title: $title
                    description: $description
                    teamId: $teamId
                }) {
                    success
                    issue {
                        identifier
                        url
                    }
                }
            }
        "#;

        let variables = serde_json::json!({
            "title": title,
            "description": body,
            "teamId": team_id
        });

        #[derive(Deserialize)]
        struct CreateResponse {
            #[serde(rename = "issueCreate")]
            issue_create: IssueCreateResult,
        }

        #[derive(Deserialize)]
        struct IssueCreateResult {
            success: bool,
            issue: Option<CreatedIssue>,
        }

        #[derive(Deserialize)]
        struct CreatedIssue {
            identifier: String,
        }

        let response: CreateResponse = self.graphql(query, Some(variables)).await?;

        if !response.issue_create.success {
            return Err(JanusError::Api("Failed to create Linear issue".to_string()));
        }

        let issue = response
            .issue_create
            .issue
            .ok_or_else(|| JanusError::Api("No issue returned from Linear".to_string()))?;

        let org = self
            .default_org
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        Ok(RemoteRef::Linear {
            org,
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

        // First, get the internal UUID for this issue
        let id_query = r#"
            query GetIssueId($identifier: String!) {
                issue(id: $identifier) {
                    id
                }
            }
        "#;

        #[derive(Deserialize)]
        struct IdResponse {
            issue: Option<IssueId>,
        }

        #[derive(Deserialize)]
        struct IssueId {
            id: String,
        }

        let id_vars = serde_json::json!({ "identifier": issue_id });
        let id_response: IdResponse = self.graphql(id_query, Some(id_vars)).await?;

        let internal_id = id_response
            .issue
            .ok_or_else(|| JanusError::RemoteIssueNotFound(remote_ref.to_string()))?
            .id;

        // Build update input
        let mut input = serde_json::Map::new();

        if let Some(title) = updates.title {
            input.insert("title".to_string(), serde_json::Value::String(title));
        }

        if let Some(body) = updates.body {
            input.insert("description".to_string(), serde_json::Value::String(body));
        }

        // Note: Updating status in Linear requires knowing the state IDs,
        // which are team-specific. For MVP, we skip status updates.
        // A full implementation would fetch available states and map them.

        if input.is_empty() {
            return Ok(());
        }

        let query = r#"
            mutation UpdateIssue($id: String!, $input: IssueUpdateInput!) {
                issueUpdate(id: $id, input: $input) {
                    success
                }
            }
        "#;

        let variables = serde_json::json!({
            "id": internal_id,
            "input": input
        });

        #[derive(Deserialize)]
        struct UpdateResponse {
            #[serde(rename = "issueUpdate")]
            issue_update: UpdateResult,
        }

        #[derive(Deserialize)]
        struct UpdateResult {
            success: bool,
        }

        let response: UpdateResponse = self.graphql(query, Some(variables)).await?;

        if !response.issue_update.success {
            return Err(JanusError::Api("Failed to update Linear issue".to_string()));
        }

        Ok(())
    }
}
