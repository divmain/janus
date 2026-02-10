//! MCP integration tests for Janus.
//!
//! These tests verify MCP functionality from the perspective of an MCP consumer
//! (like an AI coding agent). They test the full JSON-RPC flow: send a request,
//! parse the response.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

mod common;

// ============================================================================
// MCP Test Harness
// ============================================================================

/// Helper struct to interact with the MCP server process
struct McpTestClient {
    child: Child,
    stdin: std::process::ChildStdin,
    stdout_reader: BufReader<std::process::ChildStdout>,
    request_id: u64,
}

impl McpTestClient {
    /// Start the MCP server process
    fn new(working_dir: &std::path::Path) -> Self {
        let mut child = Command::new(common::janus_binary())
            .args(["mcp"])
            .current_dir(working_dir)
            .env("JANUS_SKIP_EMBEDDINGS", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to start MCP server");

        let stdin = child.stdin.take().expect("Failed to get stdin");
        let stdout = child.stdout.take().expect("Failed to get stdout");

        // Give server a moment to start
        std::thread::sleep(Duration::from_millis(100));

        McpTestClient {
            child,
            stdin,
            stdout_reader: BufReader::new(stdout),
            request_id: 0,
        }
    }

    /// Send a JSON-RPC request and read the response
    fn send_request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        self.request_id += 1;
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.request_id,
            "method": method,
            "params": params,
        });

        let request_str = serde_json::to_string(&request).unwrap();
        writeln!(self.stdin, "{request_str}").expect("Failed to write request");
        self.stdin.flush().expect("Failed to flush stdin");

        // Read response (single line)
        let mut response_line = String::new();
        self.stdout_reader
            .read_line(&mut response_line)
            .expect("Failed to read response");

        serde_json::from_str(&response_line).expect("Failed to parse response JSON")
    }

    /// Send the initialize request to properly start the MCP session
    fn initialize(&mut self) -> serde_json::Value {
        self.send_request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "test-client",
                    "version": "1.0.0"
                }
            }),
        )
    }

    /// Send the initialized notification
    fn send_initialized(&mut self) {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        });
        let notification_str = serde_json::to_string(&notification).unwrap();
        writeln!(self.stdin, "{notification_str}").expect("Failed to write notification");
        self.stdin.flush().expect("Failed to flush stdin");
    }
}

impl Drop for McpTestClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

// ============================================================================
// Initialize Tests
// ============================================================================

#[test]
fn test_mcp_initialize() {
    let janus = common::JanusTest::new();
    let mut client = McpTestClient::new(janus.temp_dir.path());

    let response = client.initialize();

    // Should have result with server info
    assert!(response["result"].is_object());
    assert_eq!(response["result"]["serverInfo"]["name"], "janus");
    assert!(response["result"]["capabilities"]["tools"].is_object());
    assert!(response["result"]["capabilities"]["resources"].is_object());
}

// ============================================================================
// Tools/List Tests
// ============================================================================

#[test]
fn test_mcp_tools_list() {
    let janus = common::JanusTest::new();
    let mut client = McpTestClient::new(janus.temp_dir.path());

    // Initialize first
    client.initialize();
    client.send_initialized();

    // List tools
    let response = client.send_request("tools/list", serde_json::json!({}));

    assert!(response["result"]["tools"].is_array());
    let tools = response["result"]["tools"].as_array().unwrap();

    // Should have 13 tools
    assert_eq!(tools.len(), 13);

    // Verify all tool names are present
    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(tool_names.contains(&"create_ticket"));
    assert!(tool_names.contains(&"spawn_subtask"));
    assert!(tool_names.contains(&"update_status"));
    assert!(tool_names.contains(&"add_note"));
    assert!(tool_names.contains(&"list_tickets"));
    assert!(tool_names.contains(&"show_ticket"));
    assert!(tool_names.contains(&"add_dependency"));
    assert!(tool_names.contains(&"remove_dependency"));
    assert!(tool_names.contains(&"add_ticket_to_plan"));
    assert!(tool_names.contains(&"get_plan_status"));
    assert!(tool_names.contains(&"get_children"));
    assert!(tool_names.contains(&"semantic_search"));
    assert!(tool_names.contains(&"get_next_available_ticket"));
}

// ============================================================================
// Resources/List Tests
// ============================================================================

#[test]
fn test_mcp_resources_list() {
    let janus = common::JanusTest::new();
    let mut client = McpTestClient::new(janus.temp_dir.path());

    // Initialize first
    client.initialize();
    client.send_initialized();

    // List resources
    let response = client.send_request("resources/list", serde_json::json!({}));

    assert!(response["result"]["resources"].is_array());
    let resources = response["result"]["resources"].as_array().unwrap();

    // Should have 5 static resources
    assert_eq!(resources.len(), 5);

    // Verify all resource URIs are present
    let uris: Vec<&str> = resources
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    assert!(uris.contains(&"janus://tickets/ready"));
    assert!(uris.contains(&"janus://tickets/blocked"));
    assert!(uris.contains(&"janus://tickets/in-progress"));
    assert!(uris.contains(&"janus://graph/deps"));
    assert!(uris.contains(&"janus://graph/spawning"));

    // Each resource should have a description
    for resource in resources {
        assert!(resource["description"].is_string());
        assert!(resource["mimeType"].is_string());
    }
}

#[test]
fn test_mcp_resource_templates_list() {
    let janus = common::JanusTest::new();
    let mut client = McpTestClient::new(janus.temp_dir.path());

    // Initialize first
    client.initialize();
    client.send_initialized();

    // List resource templates
    let response = client.send_request("resources/templates/list", serde_json::json!({}));

    assert!(response["result"]["resourceTemplates"].is_array());
    let templates = response["result"]["resourceTemplates"].as_array().unwrap();

    // Should have 4 resource templates
    assert_eq!(templates.len(), 4);

    // Verify all template URIs are present
    let uri_templates: Vec<&str> = templates
        .iter()
        .map(|t| t["uriTemplate"].as_str().unwrap())
        .collect();
    assert!(uri_templates.contains(&"janus://ticket/{id}"));
    assert!(uri_templates.contains(&"janus://plan/{id}"));
    assert!(uri_templates.contains(&"janus://plan/{id}/next"));
    assert!(uri_templates.contains(&"janus://tickets/spawned-from/{id}"));
}

// ============================================================================
// Resources/Read Tests
// ============================================================================

#[test]
fn test_mcp_read_tickets_ready() {
    let janus = common::JanusTest::new();

    // Create some tickets using CLI
    let id1 = janus
        .run_success(&["create", "Ready ticket 1"])
        .trim()
        .to_string();
    let id2 = janus
        .run_success(&["create", "Ready ticket 2"])
        .trim()
        .to_string();

    let mut client = McpTestClient::new(janus.temp_dir.path());
    client.initialize();
    client.send_initialized();

    // Read ready tickets resource
    let response = client.send_request(
        "resources/read",
        serde_json::json!({
            "uri": "janus://tickets/ready"
        }),
    );

    assert!(response["result"]["contents"].is_array());
    let contents = response["result"]["contents"].as_array().unwrap();
    assert_eq!(contents.len(), 1);
    assert_eq!(contents[0]["mimeType"], "application/json");

    // Parse the text content as JSON
    let text = contents[0]["text"].as_str().unwrap();
    let data: serde_json::Value = serde_json::from_str(text).unwrap();

    assert_eq!(data["count"], 2);
    let tickets = data["tickets"].as_array().unwrap();
    let ids: Vec<&str> = tickets.iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&id1.as_str()));
    assert!(ids.contains(&id2.as_str()));
}

#[test]
fn test_mcp_read_tickets_blocked() {
    let janus = common::JanusTest::new();

    // Create tickets and add dependency
    let dep_id = janus
        .run_success(&["create", "Dependency ticket"])
        .trim()
        .to_string();
    let blocked_id = janus
        .run_success(&["create", "Blocked ticket"])
        .trim()
        .to_string();
    janus.run_success(&["dep", "add", &blocked_id, &dep_id]);

    let mut client = McpTestClient::new(janus.temp_dir.path());
    client.initialize();
    client.send_initialized();

    // Read blocked tickets resource
    let response = client.send_request(
        "resources/read",
        serde_json::json!({
            "uri": "janus://tickets/blocked"
        }),
    );

    assert!(response["result"]["contents"].is_array());
    let contents = response["result"]["contents"].as_array().unwrap();
    let text = contents[0]["text"].as_str().unwrap();
    let data: serde_json::Value = serde_json::from_str(text).unwrap();

    assert_eq!(data["count"], 1);
    let tickets = data["tickets"].as_array().unwrap();
    assert_eq!(tickets[0]["id"], blocked_id);
    assert!(tickets[0]["blocking_deps"].is_array());
}

#[test]
fn test_mcp_read_tickets_in_progress() {
    let janus = common::JanusTest::new();

    // Create ticket and start it
    let id = janus
        .run_success(&["create", "In progress ticket"])
        .trim()
        .to_string();
    janus.run_success(&["start", &id]);

    let mut client = McpTestClient::new(janus.temp_dir.path());
    client.initialize();
    client.send_initialized();

    // Read in-progress tickets resource
    let response = client.send_request(
        "resources/read",
        serde_json::json!({
            "uri": "janus://tickets/in-progress"
        }),
    );

    let contents = response["result"]["contents"].as_array().unwrap();
    let text = contents[0]["text"].as_str().unwrap();
    let data: serde_json::Value = serde_json::from_str(text).unwrap();

    assert_eq!(data["count"], 1);
    let tickets = data["tickets"].as_array().unwrap();
    assert_eq!(tickets[0]["id"], id);
    assert_eq!(tickets[0]["status"], "in_progress");
}

#[test]
fn test_mcp_read_ticket_by_id() {
    let janus = common::JanusTest::new();

    // Create a ticket
    let id = janus
        .run_success(&["create", "Test ticket content", "-d", "Description text"])
        .trim()
        .to_string();

    let mut client = McpTestClient::new(janus.temp_dir.path());
    client.initialize();
    client.send_initialized();

    // Read the ticket resource
    let response = client.send_request(
        "resources/read",
        serde_json::json!({
            "uri": format!("janus://ticket/{}", id)
        }),
    );

    let contents = response["result"]["contents"].as_array().unwrap();
    assert_eq!(contents[0]["mimeType"], "text/markdown");

    let text = contents[0]["text"].as_str().unwrap();
    assert!(text.contains("# Test ticket content"));
    assert!(text.contains("Description text"));
    assert!(text.contains(&format!("id: {id}")));
}

#[test]
fn test_mcp_read_ticket_not_found() {
    let janus = common::JanusTest::new();

    let mut client = McpTestClient::new(janus.temp_dir.path());
    client.initialize();
    client.send_initialized();

    // Try to read non-existent ticket
    let response = client.send_request(
        "resources/read",
        serde_json::json!({
            "uri": "janus://ticket/nonexistent-id"
        }),
    );

    // Should return an error
    assert!(response["error"].is_object());
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not found"));
}

#[test]
fn test_mcp_read_spawned_from() {
    let janus = common::JanusTest::new();

    // Create parent and child tickets
    let parent_id = janus.run_success(&["create", "Parent"]).trim().to_string();
    let child1_id = janus
        .run_success(&["create", "Child 1", "--spawned-from", &parent_id])
        .trim()
        .to_string();
    let child2_id = janus
        .run_success(&["create", "Child 2", "--spawned-from", &parent_id])
        .trim()
        .to_string();

    let mut client = McpTestClient::new(janus.temp_dir.path());
    client.initialize();
    client.send_initialized();

    // Read spawned-from resource
    let response = client.send_request(
        "resources/read",
        serde_json::json!({
            "uri": format!("janus://tickets/spawned-from/{}", parent_id)
        }),
    );

    let contents = response["result"]["contents"].as_array().unwrap();
    let text = contents[0]["text"].as_str().unwrap();
    let data: serde_json::Value = serde_json::from_str(text).unwrap();

    assert_eq!(data["parent_id"], parent_id);
    assert_eq!(data["count"], 2);

    let children = data["children"].as_array().unwrap();
    let child_ids: Vec<&str> = children.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert!(child_ids.contains(&child1_id.as_str()));
    assert!(child_ids.contains(&child2_id.as_str()));
}

// ============================================================================
// Tools/Call Tests
// ============================================================================

#[test]
fn test_mcp_call_create_ticket() {
    let janus = common::JanusTest::new();

    let mut client = McpTestClient::new(janus.temp_dir.path());
    client.initialize();
    client.send_initialized();

    // Create a ticket via MCP
    let response = client.send_request(
        "tools/call",
        serde_json::json!({
            "name": "create_ticket",
            "arguments": {
                "title": "MCP Created Ticket",
                "type": "bug",
                "priority": 1
            }
        }),
    );

    assert!(response["result"]["content"].is_array());
    let content = response["result"]["content"].as_array().unwrap();
    let text = content[0]["text"].as_str().unwrap();

    // Output is now plain text: Created ticket **j-abcd**: "Title"
    assert!(
        text.contains("Created ticket"),
        "Should contain 'Created ticket'"
    );
    assert!(
        text.contains("MCP Created Ticket"),
        "Should contain the title"
    );

    // Extract ID from the response (format: Created ticket **j-xxxx**: "Title")
    let id = text
        .split("**")
        .nth(1)
        .expect("Should have ID in bold markers");
    assert!(janus.ticket_exists(id));
}

#[test]
fn test_mcp_call_list_tickets() {
    let janus = common::JanusTest::new();

    // Create some tickets
    janus.run_success(&["create", "Ticket 1"]);
    janus.run_success(&["create", "Ticket 2"]);

    let mut client = McpTestClient::new(janus.temp_dir.path());
    client.initialize();
    client.send_initialized();

    // List tickets via MCP
    let response = client.send_request(
        "tools/call",
        serde_json::json!({
            "name": "list_tickets",
            "arguments": {}
        }),
    );

    let content = response["result"]["content"].as_array().unwrap();
    let text = content[0]["text"].as_str().unwrap();

    // Verify markdown output format
    assert!(text.contains("# Tickets"), "Should have markdown header");
    assert!(
        text.contains("| ID | Title | Status | Type | Priority |"),
        "Should have table header"
    );
    assert!(
        text.contains("Ticket 1"),
        "Should contain first ticket title"
    );
    assert!(
        text.contains("Ticket 2"),
        "Should contain second ticket title"
    );
    assert!(
        text.contains("**Total:** 2 tickets"),
        "Should show total count"
    );
}

#[test]
fn test_mcp_call_update_status() {
    let janus = common::JanusTest::new();

    // Create a ticket
    let id = janus
        .run_success(&["create", "Test ticket"])
        .trim()
        .to_string();

    let mut client = McpTestClient::new(janus.temp_dir.path());
    client.initialize();
    client.send_initialized();

    // Update status via MCP
    let response = client.send_request(
        "tools/call",
        serde_json::json!({
            "name": "update_status",
            "arguments": {
                "id": id,
                "status": "in_progress"
            }
        }),
    );

    let content = response["result"]["content"].as_array().unwrap();
    let text = content[0]["text"].as_str().unwrap();

    // Output is now plain text: Updated **j-xxxx** status: new → in_progress
    assert!(text.contains("Updated"), "Should contain 'Updated'");
    assert!(text.contains("new"), "Should contain previous status");
    assert!(text.contains("in_progress"), "Should contain new status");

    // Verify status changed
    let ticket_content = janus.read_ticket(&id);
    assert!(ticket_content.contains("status: in_progress"));
}

#[test]
fn test_mcp_call_add_note() {
    let janus = common::JanusTest::new();

    // Create a ticket
    let id = janus
        .run_success(&["create", "Test ticket"])
        .trim()
        .to_string();

    let mut client = McpTestClient::new(janus.temp_dir.path());
    client.initialize();
    client.send_initialized();

    // Add note via MCP
    let response = client.send_request(
        "tools/call",
        serde_json::json!({
            "name": "add_note",
            "arguments": {
                "id": id,
                "note": "This is a test note from MCP"
            }
        }),
    );

    let content = response["result"]["content"].as_array().unwrap();
    let text = content[0]["text"].as_str().unwrap();

    // Output is now plain text: Added note to **j-xxxx** at <timestamp>
    assert!(
        text.contains("Added note to"),
        "Should contain 'Added note to'"
    );

    // Verify note was added
    let ticket_content = janus.read_ticket(&id);
    assert!(ticket_content.contains("## Notes"));
    assert!(ticket_content.contains("This is a test note from MCP"));
}

#[test]
fn test_mcp_error_response_format() {
    let janus = common::JanusTest::new();

    let mut client = McpTestClient::new(janus.temp_dir.path());
    client.initialize();
    client.send_initialized();

    // Try to update a non-existent ticket
    let response = client.send_request(
        "tools/call",
        serde_json::json!({
            "name": "update_status",
            "arguments": {
                "id": "nonexistent-id",
                "status": "in_progress"
            }
        }),
    );

    // Should have content with isError flag
    let content = response["result"]["content"].as_array().unwrap();
    assert!(
        response["result"]["isError"].as_bool().unwrap_or(false)
            || content[0]["text"].as_str().unwrap().contains("not found")
    );
}

// ============================================================================
// Plan Resource Tests
// ============================================================================

#[test]
fn test_mcp_read_plan() {
    let janus = common::JanusTest::new();

    // Create a plan
    let output = janus.run_success(&["plan", "create", "Test Plan"]);
    let plan_id = output.trim().to_string();

    // Add some tickets to the plan
    let ticket1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let ticket2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket1]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket2]);

    let mut client = McpTestClient::new(janus.temp_dir.path());
    client.initialize();
    client.send_initialized();

    // Read plan resource
    let response = client.send_request(
        "resources/read",
        serde_json::json!({
            "uri": format!("janus://plan/{}", plan_id)
        }),
    );

    let contents = response["result"]["contents"].as_array().unwrap();
    let text = contents[0]["text"].as_str().unwrap();
    let data: serde_json::Value = serde_json::from_str(text).unwrap();

    assert_eq!(data["plan_id"], plan_id);
    assert_eq!(data["title"], "Test Plan");
    assert_eq!(data["total_count"], 2);
    assert!(data["tickets"].is_array());
}

#[test]
fn test_mcp_read_plan_next() {
    let janus = common::JanusTest::new();

    // Create a plan with tickets
    let output = janus.run_success(&["plan", "create", "Test Plan"]);
    let plan_id = output.trim().to_string();

    let ticket1 = janus
        .run_success(&["create", "First task"])
        .trim()
        .to_string();
    let ticket2 = janus
        .run_success(&["create", "Second task"])
        .trim()
        .to_string();
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket1]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket2]);

    let mut client = McpTestClient::new(janus.temp_dir.path());
    client.initialize();
    client.send_initialized();

    // Read plan next resource
    let response = client.send_request(
        "resources/read",
        serde_json::json!({
            "uri": format!("janus://plan/{}/next", plan_id)
        }),
    );

    let contents = response["result"]["contents"].as_array().unwrap();
    let text = contents[0]["text"].as_str().unwrap();
    let data: serde_json::Value = serde_json::from_str(text).unwrap();

    assert_eq!(data["plan_id"], plan_id);
    assert!(data["next_items"].is_array());
}

#[test]
fn test_mcp_read_plan_not_found() {
    let janus = common::JanusTest::new();

    let mut client = McpTestClient::new(janus.temp_dir.path());
    client.initialize();
    client.send_initialized();

    // Try to read non-existent plan
    let response = client.send_request(
        "resources/read",
        serde_json::json!({
            "uri": "janus://plan/nonexistent-plan"
        }),
    );

    // Should return an error
    assert!(response["error"].is_object());
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not found"));
}

#[test]
fn test_mcp_read_unknown_resource() {
    let janus = common::JanusTest::new();

    let mut client = McpTestClient::new(janus.temp_dir.path());
    client.initialize();
    client.send_initialized();

    // Try to read unknown resource URI
    let response = client.send_request(
        "resources/read",
        serde_json::json!({
            "uri": "janus://unknown/resource"
        }),
    );

    // Should return an error
    assert!(response["error"].is_object());
}
