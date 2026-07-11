//! Plan HUD command (`janus plan hud`)
//!
//! Provides an interactive TUI heads-up display for monitoring plan progress
//! in real-time. The HUD auto-updates as ticket states change on disk.

use iocraft::prelude::*;

use crate::error::{JanusError, Result};
use crate::plan::Plan;
use crate::store::{get_or_init_store, start_watching, stop_watching};
use crate::tui::plan_hud::PlanHud;

/// Launch the plan HUD TUI
pub async fn cmd_plan_hud(plan_id: &str, bell: bool) -> Result<()> {
    // Resolve the plan ID first (before entering fullscreen) so errors display cleanly
    let plan = Plan::find(plan_id).await?;
    let resolved_id = plan.id.clone();

    // Initialize store and start filesystem watcher for live updates
    let store = get_or_init_store().await?;
    let _ = start_watching(store).await;

    let result = element!(PlanHud(plan_id: resolved_id, bell: bell))
        .fullscreen()
        .await
        .map_err(|e| JanusError::TuiError(format!("{e}")));

    // Stop the watcher to release OS-level file watch handles (FSEvents
    // streams on macOS, inotify descriptors on Linux). Without this,
    // resources accumulate across process invocations.
    stop_watching();

    result
}
