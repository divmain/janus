//! Plan show command

use crate::build_ticket_map;
use crate::cli::OutputOptions;
use crate::error::{JanusError, Result};
use crate::plan::Plan;

use super::formatters::{
    FullFormatter, JsonFormatter, PhasesOnlyFormatter, RawFormatter, TicketsOnlyFormatter,
};

/// Display a plan with full reconstruction
///
/// # Arguments
/// * `id` - The plan ID (can be partial)
/// * `raw` - If true, show raw file content instead of enhanced output
/// * `tickets_only` - If true, show only the ticket list with statuses
/// * `phases_only` - If true, show only phase summary (phased plans)
/// * `verbose_phases` - Phase numbers for which to show full completion summaries
/// * `output.json` - If true, output as JSON
pub async fn cmd_plan_show(
    id: &str,
    raw: bool,
    tickets_only: bool,
    phases_only: bool,
    verbose_phases: &[String],
    output: OutputOptions,
) -> Result<()> {
    // Validate conflicting flags
    if raw && (output.json || tickets_only || phases_only) {
        return Err(JanusError::RawWithOtherFlags);
    }

    let plan = Plan::find(id).await?;
    let metadata = plan.read()?;

    // Validate --verbose-phase usage
    if !verbose_phases.is_empty() && !metadata.is_phased() {
        return Err(JanusError::VerbosePhaseRequiresPhasedPlan);
    }

    let ticket_map = build_ticket_map().await?;

    // Delegate to appropriate formatter based on flags
    if raw {
        return RawFormatter::format(&plan);
    }

    if output.json {
        return JsonFormatter::format(&metadata, &ticket_map);
    }

    if tickets_only {
        return TicketsOnlyFormatter::format(&metadata, &ticket_map);
    }

    if phases_only {
        return PhasesOnlyFormatter::format(&metadata, &ticket_map);
    }

    // Full display (default)
    FullFormatter::format(&plan, &metadata, &ticket_map, verbose_phases)
}
