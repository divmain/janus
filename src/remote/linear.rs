//! Linear.app provider implementation using GraphQL API with type-safe cynic queries.

use reqwest::Client;

use crate::error::{JanusError, Result};

use super::{Config, IssueUpdates, Platform, RemoteIssue, RemoteProvider, RemoteRef, RemoteStatus};

const LINEAR_API_URL: &str = "https://api.linear.app/graphql";

mod graphql {
    // Re-export cynic types we need
    pub use cynic::{GraphQlResponse, MutationBuilder, QueryBuilder};

    // Import schema from the dedicated janus-schema crate.
    // The import MUST be named `schema` for cynic derives to work.
    use janus_schema::linear as schema;

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

    /// Execute a GraphQL operation (query or mutation)
    async fn execute<ResponseData, Vars>(
        &self,
        operation: cynic::Operation<ResponseData, Vars>,
    ) -> Result<ResponseData>
    where
        ResponseData: serde::de::DeserializeOwned + 'static,
        Vars: serde::Serialize,
    {
        let response = self
            .client
            .post(LINEAR_API_URL)
            .header("Authorization", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&operation)
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

        let result: GraphQlResponse<ResponseData> = response.json().await?;

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
            // If the issue is not found, return a more specific error
            if e.to_string().contains("not found") || e.to_string().contains("Entity not found") {
                JanusError::RemoteIssueNotFound(remote_ref.to_string())
            } else {
                e
            }
        })?;

        let issue = response.issue;

        // Map Linear state type to RemoteStatus
        let status = match issue.state.state_type.as_str() {
            "completed" => RemoteStatus::Closed,
            "canceled" => RemoteStatus::Custom("Cancelled".to_string()),
            _ => RemoteStatus::Custom(issue.state.name.clone()),
        };

        // Linear priority is 0-4, where 0 = no priority, 1 = urgent, 4 = low
        // We map this to our 0-4 scale where 0 = highest
        let priority = {
            let p = issue.priority as i32;
            Some(match p {
                0 => 2, // No priority -> P2 (default)
                1 => 0, // Urgent -> P0
                2 => 1, // High -> P1
                3 => 2, // Medium -> P2
                4 => 3, // Low -> P3
                _ => 4, // Other -> P4
            } as u8)
        };

        Ok(RemoteIssue {
            id: issue.identifier,
            title: issue.title,
            body: issue.description.unwrap_or_default(),
            status,
            priority,
            assignee: issue.assignee.map(|a| a.name),
            updated_at: issue.updated_at.0,
            url: issue.url,
        })
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

        // First, get the internal UUID for this issue by fetching it
        let fetch_operation = IssueQuery::build(IssueQueryVariables {
            id: issue_id.clone(),
        });

        let fetch_response = self.execute(fetch_operation).await.map_err(|e| {
            if e.to_string().contains("not found") || e.to_string().contains("Entity not found") {
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
}
