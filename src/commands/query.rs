use std::io::{BufWriter, Write, stdout};
use std::process::{Command, Stdio};

use serde_json::json;

use crate::commands::ticket_to_json;
use crate::error::{JanusError, Result};
use crate::ticket::{get_all_children_counts, get_all_tickets};

/// Enrich a ticket JSON value with its children_count from the pre-fetched map.
fn enrich_with_children_count(
    json_val: &mut serde_json::Value,
    id: &str,
    children_counts: &std::collections::HashMap<String, usize>,
) {
    let count = children_counts.get(id).copied().unwrap_or(0);
    if let serde_json::Value::Object(map) = json_val {
        map.insert("children_count".to_string(), json!(count));
    }
}

/// Write a single ticket as a JSON line to the given writer.
fn write_ticket_json(writer: &mut impl Write, json_val: &serde_json::Value) -> Result<()> {
    serde_json::to_writer(&mut *writer, json_val)?;
    writer.write_all(b"\n")?;
    Ok(())
}

/// Output tickets as JSON, optionally filtered with jq syntax
pub async fn cmd_query(filter: Option<&str>) -> Result<()> {
    let result = get_all_tickets().await?;
    let tickets = result.items;

    // Get all children counts in a single query (avoids N+1 pattern)
    let children_counts = get_all_children_counts().await?;

    if let Some(filter_expr) = filter {
        // Spawn jq to process the filter
        // NOTE: The filter expression is passed directly to the jq binary via
        // Command::args(), which does NOT perform shell interpolation. This
        // prevents shell injection attacks since arguments are passed directly
        // to the process without being interpreted by a shell.
        let filter_str = format!("select({filter_expr})");

        let mut child = Command::new("jq")
            .args(["-c", &filter_str])
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;

        // Stream each ticket as a JSON line directly to jq's stdin
        if let Some(stdin) = child.stdin.take() {
            let mut writer = BufWriter::new(stdin);
            for t in &tickets {
                let mut json_val = ticket_to_json(t);
                if let Some(id) = &t.id {
                    enrich_with_children_count(&mut json_val, id, &children_counts);
                }
                write_ticket_json(&mut writer, &json_val)?;
            }
            writer.flush()?;
            // stdin is dropped here, closing the pipe so jq can finish
        }

        let status = child.wait()?;
        if !status.success() {
            return Err(JanusError::JqFilter(format!(
                "jq filter failed with exit code {}",
                status.code().unwrap_or(-1)
            )));
        }
    } else {
        // No filter: stream each ticket as a JSON line directly to stdout
        let stdout = stdout();
        let mut writer = BufWriter::new(stdout.lock());
        for t in &tickets {
            let mut json_val = ticket_to_json(t);
            if let Some(id) = &t.id {
                enrich_with_children_count(&mut json_val, id, &children_counts);
            }
            write_ticket_json(&mut writer, &json_val)?;
        }
        writer.flush()?;
    }

    Ok(())
}
