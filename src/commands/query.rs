use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::json;

use crate::commands::ticket_to_json;
use crate::error::{JanusError, Result};
use crate::ticket::{get_all_tickets, get_children_count};

/// Output tickets as JSON, optionally filtered with jq syntax
pub async fn cmd_query(filter: Option<&str>) -> Result<()> {
    let tickets = get_all_tickets().await;

    // Build JSON lines output with children_count for each ticket
    let mut json_lines = Vec::new();
    for t in &tickets {
        let mut json_val = ticket_to_json(t);
        // Add children_count (computed on demand)
        if let Some(id) = &t.id {
            let children_count = get_children_count(id).await;
            if let serde_json::Value::Object(ref mut map) = json_val {
                map.insert("children_count".to_string(), json!(children_count));
            }
        }
        let json_str = serde_json::to_string(&json_val)
            .map_err(|e| JanusError::Other(format!("JSON serialization failed: {}", e)))?;
        json_lines.push(json_str);
    }
    let output = json_lines.join("\n");

    if let Some(filter_expr) = filter {
        // Spawn jq to process the filter
        let filter_str = format!("select({})", filter_expr);

        let mut child = Command::new("jq")
            .args(["-c", &filter_str])
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(output.as_bytes())?;
        }

        let status = child.wait()?;
        if !status.success() {
            return Err(JanusError::JqFilter(format!(
                "jq filter failed with exit code {}",
                status.code().unwrap_or(-1)
            )));
        }
    } else {
        // No filter, output all tickets as JSON lines
        println!("{}", output);
    }

    Ok(())
}
