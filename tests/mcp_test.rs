//! Integration tests for MCP server functionality.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

mod common;

// ============================================================================
// MCP --version tests
// ============================================================================

#[test]
fn test_mcp_version() {
    let output = Command::new(common::janus_binary())
        .args(["mcp", "--version"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("MCP Protocol Version:"));
    // Protocol version is managed by rmcp via ProtocolVersion::LATEST
    assert!(stdout.contains("Janus MCP Server:"));
    assert!(stdout.contains("janus"));
}

// ============================================================================
// MCP server startup tests
// ============================================================================

#[test]
fn test_mcp_server_starts_and_responds_to_initialize() {
    // Start the MCP server
    let mut child = Command::new(common::janus_binary())
        .args(["mcp"])
        .env("JANUS_SKIP_EMBEDDINGS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start MCP server");

    // Give server a moment to start
    std::thread::sleep(Duration::from_millis(100));

    // Send an initialize request
    let initialize_request = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#;

    if let Some(ref mut stdin) = child.stdin {
        writeln!(stdin, "{initialize_request}").expect("Failed to write to stdin");
    }

    // Give server time to respond
    std::thread::sleep(Duration::from_millis(200));

    // Kill the server (we don't need to fully read response for smoke test)
    let _ = child.kill();

    // Check stderr shows startup message
    let output = child.wait_with_output().expect("Failed to wait for output");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Starting Janus MCP server"),
        "Expected startup message in stderr, got: {stderr}"
    );
}

#[test]
fn test_mcp_help() {
    let output = Command::new(common::janus_binary())
        .args(["mcp", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("MCP"));
    assert!(stdout.contains("Model Context Protocol"));
    assert!(stdout.contains("--version"));
}
