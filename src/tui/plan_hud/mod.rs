//! Plan HUD view (`janus plan hud`)
//!
//! A heads-up display for monitoring plan progress in real-time.
//! Auto-updates as ticket states change on disk via the store watcher.

pub mod components;
pub mod model;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use iocraft::prelude::*;

use crate::tui::components::{
    Clickable, ModalContainer, ModalOverlay, ShortcutsBuilder, TicketDetail, Toast,
};
use crate::tui::hooks::use_store_watcher;
use crate::tui::screen_base::{ScreenLayout, should_process_key_event};
use crate::tui::services::ExternalEditor;
use crate::tui::theme::theme;
use crate::types::TicketStatus;

use components::{
    ActivityLog, DetailPanel, PhaseHeader, PhaseLayout, PlanCompleteBanner, ScrollIndicator,
    TicketRow, compute_global_bar_col, compute_phase_layout, get_flash, render_percent,
    render_progress_bar_parts,
};
use model::{
    FlashEntry, FlashType, HudState, ScrollRow, build_scroll_rows, diff_states,
    duration_since_timestamp, format_duration, load_hud_state,
};

/// Props for the PlanHud component
#[derive(Default, Props)]
pub struct PlanHudProps {
    /// The resolved plan ID
    pub plan_id: String,
    /// Whether to ring the terminal bell on completions
    pub bell: bool,
}

/// Main Plan HUD component
#[component]
pub fn PlanHud<'a>(props: &PlanHudProps, mut hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let (width, height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();

    // Core state
    let mut should_exit = hooks.use_state(|| false);
    let mut needs_reload = hooks.use_state(|| false);
    let mut is_loading = hooks.use_state(|| true);
    let hud_state: State<Option<HudState>> = hooks.use_state(|| None);
    let prev_state: State<Option<HudState>> = hooks.use_state(|| None);
    let mut toast: State<Option<Toast>> = hooks.use_state(|| None);

    // View mode
    let mut is_compact = hooks.use_state(|| false);
    let mut show_activity = hooks.use_state(|| false);
    let mut show_detail: State<Option<String>> = hooks.use_state(|| None);

    // Wide-mode detail panel: tracks which ticket to show in the right pane.
    // None means "auto-follow the active ticket". Some(id) means user override.
    let mut detail_override: State<Option<String>> = hooks.use_state(|| None);
    // Track the last known active ticket to detect changes and reset override.
    let mut last_active_id: State<Option<String>> = hooks.use_state(|| None);

    // Scroll state
    let mut scroll_offset = hooks.use_state(|| 0usize);
    let mut selected_index: State<Option<usize>> = hooks.use_state(|| None);

    // External editor deferred execution
    let mut pending_external_edit: State<Option<PathBuf>> = hooks.use_state(|| None);

    // Flash animations
    let flashes: State<HashMap<String, FlashEntry>> = hooks.use_state(HashMap::new);

    // Timer tick counter (incremented every second for time display updates)
    let tick: State<u64> = hooks.use_state(|| 0u64);

    // Track HUD start time
    let hud_start = hooks.use_state(Instant::now);

    // Bell flag from props
    let bell = props.bell;

    // Plan ID for async operations
    let plan_id = props.plan_id.clone();

    // Async load handler
    let load_handler: Handler<()> = hooks.use_async_handler({
        let plan_id = plan_id.clone();
        let mut hud_state = hud_state;
        let mut is_loading = is_loading;
        let mut prev_state = prev_state;
        let mut flashes = flashes;
        let mut toast = toast;
        move |()| {
            let plan_id = plan_id.clone();
            async move {
                match load_hud_state(&plan_id).await {
                    Ok(new_state) => {
                        // Detect transitions for flash animations
                        let current = hud_state.read().clone();
                        if let Some(ref old) = current {
                            let transitions = diff_states(old, &new_state);
                            let mut current_flashes = flashes.read().clone();
                            for (key, flash_type) in &transitions {
                                current_flashes.insert(
                                    key.clone(),
                                    FlashEntry {
                                        flash_type: *flash_type,
                                        created: Instant::now(),
                                    },
                                );
                            }
                            flashes.set(current_flashes);

                            // Terminal bell on completions
                            if bell {
                                let completion_count = transitions
                                    .iter()
                                    .filter(|(_, ft)| *ft == FlashType::Completed)
                                    .count();
                                for _ in 0..completion_count {
                                    print!("\x07");
                                }
                                // Triple bell for plan completion
                                if new_state.plan_status.status == TicketStatus::Complete
                                    && old.plan_status.status != TicketStatus::Complete
                                {
                                    print!("\x07\x07\x07");
                                }
                            }

                            // Toast for phase completions
                            for (key, flash_type) in &transitions {
                                if *flash_type == FlashType::PhaseCompleted {
                                    let phase_name = key.strip_prefix("phase-").unwrap_or(key);
                                    toast.set(Some(Toast::success(format!(
                                        "Phase {phase_name} complete!"
                                    ))));
                                }
                            }
                        }
                        prev_state.set(current);
                        hud_state.set(Some(new_state));
                    }
                    Err(e) => {
                        toast.set(Some(Toast::error(format!("Failed to load plan: {e}"))));
                    }
                }
                is_loading.set(false);
            }
        }
    });

    // Initial load
    let mut load_started = hooks.use_state(|| false);
    if !load_started.get() {
        load_started.set(true);
        load_handler.clone()(());
    }

    // Subscribe to store watcher
    hooks.use_future(use_store_watcher(needs_reload));

    // 1-second timer for time display updates + flash cleanup
    hooks.use_future({
        let mut tick = tick;
        let mut flashes = flashes;
        async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                interval.tick().await;
                tick.set(tick.get().wrapping_add(1));

                // Clean expired flashes
                let current = flashes.read().clone();
                let cleaned: HashMap<String, FlashEntry> = current
                    .into_iter()
                    .filter(|(_, entry)| {
                        entry.created.elapsed().as_secs() < model::FLASH_DURATION_SECS
                    })
                    .collect();
                flashes.set(cleaned);
            }
        }
    });

    // Reload on watcher trigger
    if needs_reload.get() && !is_loading.get() {
        needs_reload.set(false);
        is_loading.set(true);
        load_handler.clone()(());
    }

    // Deferred external editor execution
    let pending_edit_path = pending_external_edit.read().clone();
    if let Some(path) = pending_edit_path {
        pending_external_edit.set(None);
        match ExternalEditor::open_ticket_file(&path) {
            Ok(()) => {
                needs_reload.set(true);
            }
            Err(e) => {
                toast.set(Some(ExternalEditor::error_to_toast(&e)));
            }
        }
    }

    // Clipboard handler
    let copy_handler: Handler<String> = hooks.use_async_handler({
        move |ticket_id: String| {
            let mut toast = toast;
            async move {
                match clipboard_rs::ClipboardContext::new() {
                    Ok(ctx) => {
                        use clipboard_rs::Clipboard;
                        ctx.set_text(ticket_id.clone()).ok();
                        toast.set(Some(Toast::success(format!("Copied {ticket_id}"))));
                    }
                    Err(_) => {
                        toast.set(Some(Toast::error("Clipboard unavailable".to_string())));
                    }
                }
            }
        }
    });

    // Determine if we're in wide mode (>= 120 cols)
    let is_wide = width >= 120;

    // Reset detail override when active ticket changes
    {
        let current_active = hud_state
            .read()
            .as_ref()
            .and_then(|s| s.active_ticket_ids.first().cloned());
        let prev_active = last_active_id.read().clone();
        if current_active != prev_active {
            last_active_id.set(current_active);
            detail_override.set(None);
        }
    }

    // Build the flat scroll row model from the current state.
    // This flattens phase headers + tickets into a single ordered list
    // so scroll_offset can uniformly address any visible row.
    let current_scroll_rows: Vec<ScrollRow> = hud_state
        .read()
        .as_ref()
        .map(|s| build_scroll_rows(s, is_compact.get()))
        .unwrap_or_default();
    let total_rows = current_scroll_rows.len();

    // The navigable rows are only ticket rows (not phase headers).
    let total_navigable = current_scroll_rows
        .iter()
        .filter(|r| r.ticket_idx().is_some())
        .count();

    // Visible height: total height minus header (4 lines), footer (1 line), and margin.
    // We also reserve 1 line each for the ▲/▼ indicators when present.
    let base_visible_height = height.saturating_sub(7) as usize;

    // Helper: compute max scroll offset
    let max_scroll = total_rows.saturating_sub(base_visible_height);

    // Clamp scroll_offset if content shrank (e.g., compact toggle, reload)
    if scroll_offset.get() > max_scroll {
        scroll_offset.set(max_scroll);
    }

    // Clamp selected_index if it's beyond navigable range
    if let Some(sel) = selected_index.get()
        && sel >= total_navigable
        && total_navigable > 0
    {
        selected_index.set(Some(total_navigable - 1));
    }

    // Mouse scroll handlers for the left pane
    let scroll_up_handler: Handler<()> = hooks.use_async_handler({
        move |_| async move {
            scroll_offset.set(scroll_offset.get().saturating_sub(3));
        }
    });

    let scroll_down_handler: Handler<()> = hooks.use_async_handler({
        move |_| async move {
            let new_offset = (scroll_offset.get() + 3).min(max_scroll);
            scroll_offset.set(new_offset);
        }
    });

    // Click handler for ticket rows: receives the navigable index directly
    let row_click_handler: Handler<usize> = hooks.use_async_handler({
        move |nav_idx: usize| async move {
            selected_index.set(Some(nav_idx));
        }
    });

    // Keyboard event handling
    let is_showing_detail = show_detail.read().is_some();
    hooks.use_terminal_events({
        let copy_handler = copy_handler.clone();
        move |event| {
            // Helper: resolve the ticket ID for the current selection.
            // Rebuilds the navigable ticket list from the HudState on demand.
            let resolve_selected_ticket_id = |state: &HudState| -> Option<String> {
                let rows = build_scroll_rows(state, is_compact.get());
                let nav_ticket_ids: Vec<String> = rows
                    .iter()
                    .filter_map(|r| r.ticket_idx())
                    .filter_map(|idx| state.tickets.get(idx).map(|t| t.id.clone()))
                    .collect();
                selected_index
                    .get()
                    .and_then(|nav_idx| nav_ticket_ids.get(nav_idx).cloned())
                    .or_else(|| state.active_ticket_ids.first().cloned())
            };

            // Helper: get the scroll row index for a given navigable index
            let nav_to_scroll_row = |state: &HudState, nav_idx: usize| -> usize {
                let rows = build_scroll_rows(state, is_compact.get());
                rows.iter()
                    .enumerate()
                    .filter(|(_, r)| r.ticket_idx().is_some())
                    .nth(nav_idx)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            };

            match event {
                TerminalEvent::Key(KeyEvent {
                    code,
                    kind,
                    modifiers,
                    ..
                }) if should_process_key_event(kind) => {
                    // Detail modal: Esc closes it
                    if is_showing_detail {
                        match code {
                            KeyCode::Esc | KeyCode::Char('q') => {
                                show_detail.set(None);
                            }
                            _ => {}
                        }
                        return;
                    }

                    match code {
                        // Quit
                        KeyCode::Char('q') => should_exit.set(true),
                        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                            should_exit.set(true);
                        }

                        // Toggle compact mode
                        KeyCode::Tab => {
                            is_compact.set(!is_compact.get());
                        }

                        // Toggle activity log
                        KeyCode::Char('a') => {
                            show_activity.set(!show_activity.get());
                        }

                        // Navigation — operates on navigable (ticket) rows only
                        KeyCode::Char('j') | KeyCode::Down => {
                            if total_navigable == 0 {
                                return;
                            }
                            let current = selected_index.get().unwrap_or(0);
                            if current + 1 < total_navigable {
                                let new_nav = current + 1;
                                selected_index.set(Some(new_nav));
                                // Auto-scroll to keep selection visible
                                if let Some(ref state) = *hud_state.read() {
                                    let scroll_row_idx = nav_to_scroll_row(state, new_nav);
                                    if scroll_row_idx >= scroll_offset.get() + base_visible_height {
                                        scroll_offset.set(
                                            scroll_row_idx.saturating_sub(base_visible_height - 1),
                                        );
                                    }
                                }
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            if total_navigable == 0 {
                                return;
                            }
                            let current = selected_index.get().unwrap_or(0);
                            if current > 0 {
                                let new_nav = current - 1;
                                selected_index.set(Some(new_nav));
                                if let Some(ref state) = *hud_state.read() {
                                    let scroll_row_idx = nav_to_scroll_row(state, new_nav);
                                    if scroll_row_idx < scroll_offset.get() {
                                        scroll_offset.set(scroll_row_idx);
                                    }
                                }
                            }
                        }
                        KeyCode::Char('g') => {
                            selected_index.set(Some(0));
                            scroll_offset.set(0);
                        }
                        KeyCode::Char('G') => {
                            if total_navigable > 0 {
                                selected_index.set(Some(total_navigable - 1));
                                if let Some(ref state) = *hud_state.read() {
                                    let scroll_row_idx =
                                        nav_to_scroll_row(state, total_navigable - 1);
                                    scroll_offset.set(
                                        scroll_row_idx
                                            .saturating_sub(base_visible_height.saturating_sub(1)),
                                    );
                                }
                            }
                        }
                        KeyCode::PageDown => {
                            if total_navigable == 0 {
                                return;
                            }
                            let page = base_visible_height.max(1);
                            let current = selected_index.get().unwrap_or(0);
                            let new_nav = (current + page).min(total_navigable.saturating_sub(1));
                            selected_index.set(Some(new_nav));
                            scroll_offset.set((scroll_offset.get() + page).min(max_scroll));
                        }
                        KeyCode::PageUp => {
                            if total_navigable == 0 {
                                return;
                            }
                            let page = base_visible_height.max(1);
                            let current = selected_index.get().unwrap_or(0);
                            let new_nav = current.saturating_sub(page);
                            selected_index.set(Some(new_nav));
                            scroll_offset.set(scroll_offset.get().saturating_sub(page));
                        }

                        // Copy active ticket ID
                        KeyCode::Char('c') => {
                            if let Some(ref state) = *hud_state.read()
                                && let Some(id) = resolve_selected_ticket_id(state)
                            {
                                copy_handler.clone()(id);
                            }
                        }

                        // Open ticket detail (modal in narrow, panel override in wide)
                        KeyCode::Enter => {
                            if let Some(ref state) = *hud_state.read()
                                && let Some(id) = resolve_selected_ticket_id(state)
                            {
                                if is_wide {
                                    detail_override.set(Some(id));
                                } else {
                                    show_detail.set(Some(id));
                                }
                            }
                        }

                        // Open ticket file in $EDITOR
                        KeyCode::Char('E') => {
                            if let Some(ref state) = *hud_state.read()
                                && let Some(id) = resolve_selected_ticket_id(state)
                                && let Some(ticket) = state.tickets.iter().find(|t| t.id == id)
                                && let Some(ref metadata) = ticket.metadata
                                && let Some(ref file_path) = metadata.file_path
                            {
                                // Close detail modal if open (narrow mode)
                                show_detail.set(None);
                                pending_external_edit.set(Some(file_path.clone()));
                            }
                        }

                        // Reset detail panel to follow active ticket (wide mode)
                        KeyCode::Esc => {
                            if is_wide {
                                detail_override.set(None);
                            }
                        }

                        _ => {}
                    }
                }
                _ => {}
            }
        }
    });

    // Handle exit
    if should_exit.get() {
        system.exit();
    }

    let theme = theme();
    let _tick_val = tick.get(); // force re-render on tick

    // Read flash state
    let flash_map = flashes.read().clone();

    // Build shortcuts
    let shortcuts = if is_showing_detail {
        ShortcutsBuilder::new().add("Esc", "Close").build()
    } else if is_wide {
        let mut builder = ShortcutsBuilder::new()
            .add("j/k", "Navigate")
            .add("Enter", "Show Ticket")
            .add("Esc", "Follow Active")
            .add("E", "$EDITOR")
            .add("c", "Copy ID")
            .add("Tab", "Compact");
        builder = builder.add("q", "Quit");
        builder.build()
    } else {
        ShortcutsBuilder::new()
            .add("j/k", "Navigate")
            .add("Enter", "Detail")
            .add("E", "$EDITOR")
            .add("c", "Copy ID")
            .add("Tab", "Compact")
            .add("a", "Activity")
            .add("q", "Quit")
            .build()
    };

    // Read hud state
    let state_ref = hud_state.read();
    let state = state_ref.as_ref();

    // Build the header info
    let plan_title = state
        .map(|s| {
            s.plan
                .title
                .as_deref()
                .unwrap_or("Untitled Plan")
                .to_string()
        })
        .unwrap_or_else(|| "Loading...".to_string());

    let plan_id_display = props.plan_id.clone();

    let plan_status = state.map(|s| s.plan_status.clone());
    let completed = plan_status.as_ref().map(|s| s.completed_count).unwrap_or(0);
    let total = plan_status.as_ref().map(|s| s.total_count).unwrap_or(0);

    // Elapsed time since HUD start
    let elapsed = hud_start.get().elapsed();
    let elapsed_str = format_duration(elapsed);

    // Active ticket timing
    let active_time_str = state
        .and_then(|s| s.timing.active_ticket_start.as_ref())
        .and_then(|ts| duration_since_timestamp(ts))
        .map(format_duration);

    // Estimated remaining
    let est_remaining_str = state
        .and_then(|s| s.timing.estimated_remaining())
        .map(|d| format!("~{}", format_duration(d)));

    // Resolve which ticket to show in the wide-mode detail panel
    let panel_ticket_id: Option<String> = detail_override
        .read()
        .clone()
        .or_else(|| state.and_then(|s| s.active_ticket_ids.first().cloned()));
    let panel_ticket = panel_ticket_id.as_ref().and_then(|id| {
        state.and_then(|s| {
            s.tickets
                .iter()
                .find(|t| t.id == *id)
                .and_then(|t| t.metadata.clone())
        })
    });
    let panel_body = panel_ticket_id
        .as_ref()
        .and_then(|id| {
            state.and_then(|s| {
                s.tickets
                    .iter()
                    .find(|t| t.id == *id)
                    .and_then(|t| t.metadata.as_ref().and_then(|m| m.body.clone()))
            })
        })
        .unwrap_or_default();

    // Determine active time string for the panel ticket
    let panel_active_time =
        if panel_ticket_id.as_ref() == state.and_then(|s| s.active_ticket_ids.first()) {
            active_time_str.clone()
        } else {
            None
        };

    // Activity events for detail panel
    let panel_events = state.map(|s| s.activity_events.clone()).unwrap_or_default();

    // Progress bar for header
    let bar_width = 20.min(width.saturating_sub(60) as usize).max(8);
    let (header_bar_filled, header_bar_empty) =
        render_progress_bar_parts(completed, total, bar_width);
    let progress_pct = render_percent(completed, total);

    // Plan complete?
    let plan_complete = plan_status
        .as_ref()
        .is_some_and(|s| s.status == TicketStatus::Complete && total > 0);

    // Build header line
    let header_line = format!("{plan_title} ({plan_id_display})");
    let progress_line_suffix = {
        let mut parts = vec![
            format!(" {progress_pct} ({completed}/{total})"),
            format!("Elapsed: {elapsed_str}"),
        ];
        if let Some(ref est) = est_remaining_str {
            parts.push(format!("Est: {est}"));
        }
        parts.join("  ")
    };

    // Determine activity log height
    let activity_height = if show_activity.get() { 8u32 } else { 0u32 };

    // For the detail ticket body, read from store if needed
    let detail_ticket = show_detail.read().clone().and_then(|id| {
        state.and_then(|s| {
            s.tickets
                .iter()
                .find(|t| t.id == id)
                .and_then(|t| t.metadata.clone())
        })
    });
    let detail_body = show_detail
        .read()
        .clone()
        .and_then(|id| {
            state.and_then(|s| {
                s.tickets
                    .iter()
                    .find(|t| t.id == id)
                    .and_then(|t| t.metadata.as_ref().and_then(|m| m.body.clone()))
            })
        })
        .unwrap_or_default();

    // Drop the read guard before rendering
    drop(state_ref);

    // Re-acquire for rendering
    let state_ref = hud_state.read();
    let state = state_ref.as_ref();

    // Pre-compute layout for aligned columns
    let (phase_layouts, global_bar_col) = if let Some(state) = state {
        if state.is_simple {
            // Simple plan: one layout for all tickets
            let all_refs: Vec<&model::HudTicket> = state.tickets.iter().collect();
            let pane_w = if is_wide {
                (width as usize) * 58 / 100
            } else {
                width as usize
            };
            let layout = compute_phase_layout(&all_refs, pane_w);
            (vec![layout], None)
        } else {
            // Phased plan: one layout per phase, plus global bar column
            let phases = state.plan.phases();
            let pane_w = if is_wide {
                (width as usize) * 58 / 100
            } else {
                width as usize
            };

            // Compute phase header labels for bar alignment
            let phase_labels: Vec<String> = phases
                .iter()
                .map(|p| format!("Phase {}: {}", p.number, p.name))
                .collect();
            let bar_col = compute_global_bar_col(&phase_labels);

            // Compute per-phase ticket layouts
            let layouts: Vec<PhaseLayout> = state
                .phase_tickets
                .iter()
                .map(|indices| {
                    let ticket_refs: Vec<&model::HudTicket> = indices
                        .iter()
                        .filter_map(|&idx| state.tickets.get(idx))
                        .collect();
                    compute_phase_layout(&ticket_refs, pane_w)
                })
                .collect();

            (layouts, Some(bar_col))
        }
    } else {
        (vec![], None)
    };

    // Re-build the scroll rows for rendering (need the state ref for rendering).
    // We rebuild here because the earlier scroll_rows was built before we dropped
    // the read guard and re-acquired it. This ensures consistency.
    let render_scroll_rows: Vec<ScrollRow> = state
        .map(|s| build_scroll_rows(s, is_compact.get()))
        .unwrap_or_default();
    let render_total_rows = render_scroll_rows.len();

    // Scroll visibility: which rows are visible in the viewport
    let scroll_off = scroll_offset.get();
    let has_more_above = scroll_off > 0;
    let has_more_below = scroll_off + base_visible_height < render_total_rows;

    // Account for indicator lines stealing space from visible rows
    let indicator_overhead = has_more_above as usize + has_more_below as usize;
    let content_visible_height = base_visible_height.saturating_sub(indicator_overhead);

    // Slice the visible rows
    let visible_end = (scroll_off + content_visible_height).min(render_total_rows);
    let visible_rows = &render_scroll_rows[scroll_off..visible_end];

    // Build navigable index lookup for the *rendered* slice (for is_selected checks).
    // We need to map from a scroll_row's ticket_idx back to its navigable index.
    let navigable_for_render: HashMap<usize, usize> = {
        // Rebuild navigable_indices from scroll_rows for the render pass
        let nav: Vec<usize> = render_scroll_rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| row.ticket_idx().map(|_| i))
            .collect();
        nav.iter()
            .enumerate()
            .filter_map(|(nav_idx, &scroll_row_idx)| {
                render_scroll_rows
                    .get(scroll_row_idx)
                    .and_then(|r| r.ticket_idx())
                    .map(|ticket_idx| (ticket_idx, nav_idx))
            })
            .collect()
    };

    // Build the visible row elements
    let pane_width = if is_wide {
        width as u32 * 58 / 100
    } else {
        width as u32
    };

    let row_elements: Vec<AnyElement<'static>> = if let Some(state) = state {
        visible_rows
            .iter()
            .map(|row| match row {
                ScrollRow::PhaseHeader { phase_idx } => {
                    let phases = state.plan.phases();
                    let phase = &phases[*phase_idx];
                    let ps = state.phase_statuses.get(*phase_idx);
                    let phase_status = ps.map(|p| p.status).unwrap_or(TicketStatus::New);
                    let phase_completed = ps.map(|p| p.completed_count).unwrap_or(0);
                    let phase_total = ps.map(|p| p.total_count).unwrap_or(0);
                    let is_active_phase = state.phase_statuses.iter().take(*phase_idx).all(|p| {
                        p.status == TicketStatus::Complete || p.status == TicketStatus::Cancelled
                    }) && phase_status != TicketStatus::Complete
                        && phase_status != TicketStatus::Cancelled;
                    let phase_flash = get_flash(&flash_map, &format!("phase-{}", phase.number));

                    element! {
                        PhaseHeader(
                            number: phase.number.clone(),
                            name: phase.name.clone(),
                            status: Some(phase_status),
                            completed: phase_completed,
                            total: phase_total,
                            is_active_phase: is_active_phase,
                            flash: phase_flash,
                            width: pane_width,
                            bar_start_col: global_bar_col,
                        )
                    }
                    .into()
                }
                ScrollRow::Ticket { ticket_idx } => {
                    let ticket = state.tickets.get(*ticket_idx);
                    let nav_idx = navigable_for_render.get(ticket_idx).copied();
                    let is_selected = nav_idx.is_some() && selected_index.get() == nav_idx;
                    let flash = ticket.map(|t| get_flash(&flash_map, &t.id)).unwrap_or(None);

                    // Determine which phase layout to use
                    let ticket_layout = if state.is_simple {
                        phase_layouts.first().cloned()
                    } else {
                        // Find the phase that contains this ticket
                        state
                            .phase_tickets
                            .iter()
                            .position(|indices| indices.contains(ticket_idx))
                            .and_then(|idx| phase_layouts.get(idx).cloned())
                    };

                    let click_nav_idx = nav_idx.unwrap_or(0);
                    let click_handler = row_click_handler.clone();

                    element! {
                        Clickable(
                            on_click: Some(Handler::from(move |_| {
                                click_handler(click_nav_idx);
                            })),
                        ) {
                            TicketRow(
                                ticket: ticket.cloned(),
                                is_selected: is_selected,
                                flash: flash,
                                width: pane_width,
                                layout: ticket_layout,
                            )
                        }
                    }
                    .into()
                }
            })
            .collect()
    } else {
        vec![]
    };

    let has_content = state.is_some() && !row_elements.is_empty();

    element! {
        ScreenLayout(
            width: width,
            height: height,
            header_title: Some("Janus - Plan HUD"),
            header_ticket_count: Some(total),
            shortcuts: shortcuts,
            toast: toast.read().clone(),
        ) {
            // Header section: plan title + progress
            View(
                width: 100pct,
                flex_direction: FlexDirection::Column,
                padding_left: 1,
                padding_right: 1,
            ) {
                // Plan title
                View(height: 1, width: 100pct) {
                    Text(content: header_line, color: theme.text, weight: Weight::Bold)
                }
                // Progress line
                View(height: 1, width: 100pct, flex_direction: FlexDirection::Row) {
                    Text(content: header_bar_filled.clone(), color: theme.status_complete)
                    Text(content: header_bar_empty.clone(), color: theme.status_in_progress)
                    Text(content: progress_line_suffix.clone(), color: theme.text_dimmed)
                }
            }

            // Plan Complete banner (if applicable)
            #(if plan_complete {
                Some(element! {
                    PlanCompleteBanner(
                        plan_title: plan_title.clone(),
                        total_tickets: total,
                        elapsed: Some(elapsed_str.clone()),
                    )
                })
            } else {
                None
            })

            // Main content area
            #(if !plan_complete {
                if is_wide {
                    // Wide mode: two-pane layout (phases left, detail+activity right)
                    Some(element! {
                        View(
                            flex_grow: 1.0,
                            width: 100pct,
                            flex_direction: FlexDirection::Row,
                            overflow: Overflow::Hidden,
                        ) {
                            // Left pane: scrollable phase/ticket list (~58%)
                            Clickable(
                                on_scroll_up: Some(scroll_up_handler.clone()),
                                on_scroll_down: Some(scroll_down_handler.clone()),
                            ) {
                                View(
                                    width: 58pct,
                                    height: 100pct,
                                    flex_shrink: 0.0,
                                    flex_direction: FlexDirection::Column,
                                    overflow: Overflow::Hidden,
                                    padding_left: 1,
                                    padding_right: 1,
                                ) {
                                    #(if has_content {
                                        Some(element! {
                                            View(
                                                flex_grow: 1.0,
                                                width: 100pct,
                                                flex_direction: FlexDirection::Column,
                                                overflow: Overflow::Hidden,
                                            ) {
                                                // ▲ indicator
                                                ScrollIndicator(
                                                    has_more_above: has_more_above,
                                                    has_more_below: has_more_below,
                                                    is_above: true,
                                                    width: pane_width,
                                                )
                                                // Visible rows
                                                #(row_elements)
                                                // ▼ indicator
                                                ScrollIndicator(
                                                    has_more_above: has_more_above,
                                                    has_more_below: has_more_below,
                                                    is_above: false,
                                                    width: pane_width,
                                                )
                                            }
                                        })
                                    } else if state.is_none() {
                                        Some(element! {
                                            View(
                                                flex_grow: 1.0,
                                                width: 100pct,
                                                align_items: AlignItems::Center,
                                                justify_content: JustifyContent::Center,
                                            ) {
                                                Text(content: "Loading plan...", color: theme.text_dimmed)
                                            }
                                        })
                                    } else {
                                        None
                                    })
                                }
                            }

                            // Right pane: detail + activity (~42%)
                            View(
                                flex_grow: 1.0,
                                height: 100pct,
                            ) {
                                DetailPanel(
                                    ticket: panel_ticket.clone(),
                                    body: panel_body.clone(),
                                    events: panel_events.clone(),
                                    active_time_str: panel_active_time.clone(),
                                )
                            }
                        }
                    })
                } else {
                    // Narrow mode: single column scrollable layout
                    Some(element! {
                        View(
                            flex_grow: 1.0,
                            width: 100pct,
                            flex_direction: FlexDirection::Column,
                            overflow: Overflow::Hidden,
                        ) {
                            Clickable(
                                on_scroll_up: Some(scroll_up_handler.clone()),
                                on_scroll_down: Some(scroll_down_handler.clone()),
                            ) {
                                View(
                                    flex_grow: 1.0,
                                    width: 100pct,
                                    flex_direction: FlexDirection::Column,
                                    overflow: Overflow::Hidden,
                                    padding_left: 1,
                                    padding_right: 1,
                                ) {
                                    #(if has_content {
                                        Some(element! {
                                            View(
                                                flex_grow: 1.0,
                                                width: 100pct,
                                                flex_direction: FlexDirection::Column,
                                                overflow: Overflow::Hidden,
                                            ) {
                                                // ▲ indicator
                                                ScrollIndicator(
                                                    has_more_above: has_more_above,
                                                    has_more_below: has_more_below,
                                                    is_above: true,
                                                    width: pane_width,
                                                )
                                                // Visible rows
                                                #(row_elements)
                                                // ▼ indicator
                                                ScrollIndicator(
                                                    has_more_above: has_more_above,
                                                    has_more_below: has_more_below,
                                                    is_above: false,
                                                    width: pane_width,
                                                )
                                            }
                                        })
                                    } else if state.is_none() {
                                        Some(element! {
                                            View(
                                                flex_grow: 1.0,
                                                width: 100pct,
                                                align_items: AlignItems::Center,
                                                justify_content: JustifyContent::Center,
                                            ) {
                                                Text(content: "Loading plan...", color: theme.text_dimmed)
                                            }
                                        })
                                    } else {
                                        None
                                    })
                                }
                            }
                        }
                    })
                }
            } else {
                None
            })

            // Activity log (toggleable, narrow mode only)
            #(if show_activity.get() && !is_wide {
                let events = state
                    .map(|s| s.activity_events.clone())
                    .unwrap_or_default();
                Some(element! {
                    ActivityLog(
                        events: events,
                        height: activity_height,
                        width: width as u32,
                    )
                })
            } else {
                None
            })

            // Ticket detail modal (narrow mode only)
            #(if !is_wide {
                (*show_detail.read()).as_ref().map(|_detail_id| element! {
                    ModalOverlay() {
                        ModalContainer(
                            title: Some("Ticket Detail".to_string()),
                            width: Some(crate::tui::components::ModalWidth::Percent(80)),
                            height: Some(crate::tui::components::ModalHeight::Percent(80)),
                        ) {
                            TicketDetail(
                                ticket: detail_ticket.clone(),
                                body: detail_body.clone(),
                                has_focus: true,
                                scroll_offset: 0usize,
                            )
                        }
                    }
                })
            } else {
                None
            })
        }
    }
}
