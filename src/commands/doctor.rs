//! Doctor command
//!
//! Scans all ticket files and reports any that failed to load or parse.
//! Similar to `janus plan verify` but for tickets.

use owo_colors::OwoColorize;
use serde_json::json;

use crate::cli::OutputOptions;
use crate::commands::CommandOutput;
use crate::error::Result;
use crate::ticket::get_all_tickets_from_disk;

/// Verify all ticket files and report any failures
///
/// # Arguments
/// * `output` - Output options for controlling JSON output
///
/// # Returns
/// Returns Ok(()) with exit code 0 if all tickets are valid,
/// or Ok(()) with failures printed if there are issues.
/// Callers should check the failure count to determine exit code.
pub fn cmd_doctor(output: OutputOptions) -> Result<(bool, Vec<(String, String)>)> {
    let result = get_all_tickets_from_disk();

    let success_count = result.success_count();
    let failure_count = result.failure_count();
    let failures: Vec<(String, String)> = result.failed.clone();

    // Build JSON output
    let json_output = json!({
        "valid": failure_count == 0,
        "success_count": success_count,
        "failure_count": failure_count,
        "failures": result.failed.iter().map(|(f, e)| json!({
            "file": f,
            "error": e,
        })).collect::<Vec<_>>(),
    });

    // Build text output
    let mut text_output = String::new();

    text_output.push_str(&format!("\n{}\n", "Doctor - Ticket Health Check".bold()));
    text_output.push_str(&format!("{}\n", "==============================".bold()));
    text_output.push('\n');
    text_output.push_str(&format!(
        "{} valid ticket(s) found\n",
        success_count.to_string().green()
    ));

    if failure_count > 0 {
        text_output.push_str(&format!(
            "{} ticket file(s) with errors:\n\n",
            failure_count.to_string().red()
        ));
        for (file, error) in &result.failed {
            text_output.push_str(&format!("  {} {}\n", "✗".red(), file.cyan()));
            text_output.push_str(&format!("    {}\n\n", error.dimmed()));
        }
    } else {
        text_output.push_str(&format!("\n{} All ticket files are valid!", "✓".green()));
    }

    CommandOutput::new(json_output)
        .with_text(text_output)
        .print(output)?;

    Ok((failure_count == 0, failures))
}
