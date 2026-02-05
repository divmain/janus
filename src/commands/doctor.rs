//! Doctor command
//!
//! Scans all ticket files and reports any that failed to load or parse.
//! Similar to `janus plan verify` but for tickets.

use owo_colors::OwoColorize;
use serde_json::json;

use crate::commands::CommandOutput;
use crate::error::Result;
use crate::ticket::get_all_tickets_from_disk;

/// Verify all ticket files and report any failures
///
/// # Arguments
/// * `output_json` - If true, output as JSON
///
/// # Returns
/// Returns Ok(()) with exit code 0 if all tickets are valid,
/// or Ok(()) with failures printed if there are issues.
/// Callers should check the failure count to determine exit code.
pub fn cmd_doctor(output_json: bool) -> Result<(bool, Vec<(String, String)>)> {
    let result = get_all_tickets_from_disk();

    let success_count = result.success_count();
    let failure_count = result.failure_count();
    let failures: Vec<(String, String)> = result.failed.clone();

    if output_json {
        let json_output = json!({
            "valid": failure_count == 0,
            "success_count": success_count,
            "failure_count": failure_count,
            "failures": result.failed.iter().map(|(f, e)| json!({
                "file": f,
                "error": e,
            })).collect::<Vec<_>>(),
        });
        let _ = CommandOutput::new(json_output.clone())
            .with_text(format!("{json_output}"))
            .print(true);
    } else {
        println!("\n{}", "Doctor - Ticket Health Check".bold());
        println!("{}", "==============================".bold());
        println!();
        println!(
            "{} valid ticket(s) found",
            success_count.to_string().green()
        );

        if failure_count > 0 {
            println!(
                "{} ticket file(s) with errors:\n",
                failure_count.to_string().red()
            );
            for (file, error) in &result.failed {
                println!("  {} {}", "✗".red(), file.cyan());
                println!("    {}\n", error.dimmed());
            }
        } else {
            println!("\n{} All ticket files are valid!", "✓".green());
        }
    }

    Ok((failure_count == 0, failures))
}
