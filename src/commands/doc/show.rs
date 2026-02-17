use owo_colors::OwoColorize;

use crate::cli::OutputOptions;
use crate::commands::print_json;
use crate::doc::Doc;
use crate::error::Result;

/// Display a document with optional line range
pub async fn cmd_doc_show(label: &str, lines: Option<String>, output: OutputOptions) -> Result<()> {
    let doc = Doc::find(label).await?;
    let content = doc.read_content()?;
    let metadata = doc.read()?;

    // Parse line range if provided
    let line_range = if let Some(range_str) = lines {
        Some(parse_line_range(&range_str)?)
    } else {
        None
    };

    if output.json {
        let json_output = serde_json::json!({
            "label": doc.label,
            "title": metadata.title(),
            "description": metadata.description,
            "tags": metadata.tags,
            "created": metadata.created.as_ref().map(|c| c.to_string()),
            "updated": metadata.updated.as_ref().map(|c| c.to_string()),
            "content": content,
            "file_path": doc.file_path.to_string_lossy().to_string(),
        });
        print_json(&json_output)?;
    } else {
        // Display metadata header
        println!("{}: {}", "Label".green().bold(), doc.label);
        if let Some(title) = metadata.title() {
            println!("{}: {}", "Title".green().bold(), title);
        }
        if let Some(description) = &metadata.description {
            println!("{}: {}", "Description".green().bold(), description);
        }
        if !metadata.tags.is_empty() {
            println!("{}: {}", "Tags".green().bold(), metadata.tags.join(", "));
        }
        println!();

        // Display content (with optional line range)
        if let Some((start, end)) = line_range {
            let lines: Vec<&str> = content.lines().collect();
            let start = start.saturating_sub(1); // Convert to 0-indexed
            let end = end.min(lines.len());

            if start < lines.len() {
                for (i, line) in lines[start..end].iter().enumerate() {
                    let line_num = start + i + 1;
                    println!("{line_num:4} │ {line}");
                }
            }
        } else {
            println!("{content}");
        }
    }

    Ok(())
}

/// Parse a line range string like "10-50" or "5"
fn parse_line_range(range_str: &str) -> Result<(usize, usize)> {
    let range_str = range_str.trim();

    if let Some(dash_pos) = range_str.find('-') {
        // Range format: "start-end"
        let start_str = &range_str[..dash_pos].trim();
        let end_str = &range_str[dash_pos + 1..].trim();

        let start: usize = start_str.parse().map_err(|_| {
            crate::error::JanusError::InvalidInput(format!("Invalid line number: '{start_str}'"))
        })?;
        let end: usize = end_str.parse().map_err(|_| {
            crate::error::JanusError::InvalidInput(format!("Invalid line number: '{end_str}'"))
        })?;

        if start > end {
            return Err(crate::error::JanusError::InvalidInput(format!(
                "Invalid line range: start ({start}) must be <= end ({end})"
            )));
        }

        Ok((start, end))
    } else {
        // Single line format: "N"
        let line: usize = range_str.parse().map_err(|_| {
            crate::error::JanusError::InvalidInput(format!("Invalid line number: '{range_str}'"))
        })?;
        Ok((line, line))
    }
}
