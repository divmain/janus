//! Sub-components for the Plan HUD
//!
//! Reusable rendering components: progress bar, phase section, ticket row.

use std::collections::HashMap;

use iocraft::prelude::*;

use crate::tui::theme::theme;
use crate::types::TicketStatus;

use super::model::{
    ActivityEvent, FLASH_DURATION_SECS, FlashEntry, FlashType, HudTicket, format_duration,
};

// ============================================================================
// Progress Bar
// ============================================================================

/// Render the filled and empty portions of a progress bar as separate strings.
/// Both use the `░` character, differentiated by color at the call site.
pub fn render_progress_bar_parts(completed: usize, total: usize, width: usize) -> (String, String) {
    if total == 0 {
        return (String::new(), "░".repeat(width));
    }
    let filled = ((completed as f64 / total as f64) * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;
    ("░".repeat(filled), "░".repeat(empty))
}

/// Render a percentage string
pub fn render_percent(completed: usize, total: usize) -> String {
    if total == 0 {
        return "0%".to_string();
    }
    let pct = (completed as f64 / total as f64) * 100.0;
    format!("{:.0}%", pct)
}

// ============================================================================
// Layout Computation
// ============================================================================

/// Pre-computed column widths for aligned ticket rows within a phase
#[derive(Debug, Clone, Default)]
pub struct PhaseLayout {
    /// Width of the ID column (max ticket ID length + 1 space)
    pub id_col_width: usize,
    /// Width of the right column (max of annotation or duration strings + 1 space)
    pub right_col_width: usize,
    /// Available width for the title (pane_width - indent - id_col - right_col)
    pub title_max_width: usize,
    /// Indent for continuation lines (icon(2) + padding(2) + id_col)
    pub continuation_indent: usize,
}

const ICON_WIDTH: usize = 2; // "✓ " or "● "
const ROW_PADDING_LEFT: usize = 2; // padding_left: 2

/// Compute the annotation string for a ticket (without leading space)
pub fn ticket_annotation(ticket: &HudTicket) -> String {
    if ticket.is_active {
        "◀ NOW".to_string()
    } else {
        String::new()
    }
}

/// Compute the right-column string for a ticket (annotation or duration)
pub fn ticket_right_text(ticket: &HudTicket) -> String {
    let ann = ticket_annotation(ticket);
    if !ann.is_empty() {
        return ann;
    }
    ticket
        .completion_duration
        .map(format_duration)
        .unwrap_or_default()
}

/// Compute layout for a set of tickets within a phase
pub fn compute_phase_layout(tickets: &[&HudTicket], pane_width: usize) -> PhaseLayout {
    // Max ID width
    let id_col_width = tickets.iter().map(|t| t.id.len()).max().unwrap_or(8) + 1; // +1 for trailing space

    // Max right column width (annotation or duration)
    let right_col_width = tickets
        .iter()
        .map(|t| ticket_right_text(t).len())
        .max()
        .unwrap_or(0)
        .max(1); // at least 1 to avoid zero-width

    let fixed_cols = ROW_PADDING_LEFT + ICON_WIDTH + id_col_width + right_col_width + 1; // +1 spacing
    let title_max_width = pane_width.saturating_sub(fixed_cols).max(10);
    let continuation_indent = ROW_PADDING_LEFT + ICON_WIDTH + id_col_width;

    PhaseLayout {
        id_col_width,
        right_col_width,
        title_max_width,
        continuation_indent,
    }
}

/// Compute the column where phase header progress bars should start, globally aligned.
/// Returns the number of characters from the left edge where the bar should begin.
pub fn compute_global_bar_col(
    phase_labels: &[String], // e.g., ["Phase 1: Test Infrastructure", "Phase 2: Core Navigation"]
) -> usize {
    // icon(2) + longest_label + 1 space
    let max_label_len = phase_labels.iter().map(|l| l.len()).max().unwrap_or(20);
    ICON_WIDTH + max_label_len + 1
}

// ============================================================================
// Status Icons
// ============================================================================

/// Get the status icon character for a ticket status
pub fn status_icon(status: TicketStatus) -> &'static str {
    match status {
        TicketStatus::Complete => "✓",
        TicketStatus::InProgress => "●",
        TicketStatus::Next => "◎",
        TicketStatus::New => "○",
        TicketStatus::Cancelled => "✗",
        TicketStatus::Archived => "▣",
    }
}

/// Get the phase status icon
pub fn phase_status_icon(status: TicketStatus) -> &'static str {
    match status {
        TicketStatus::Complete => "✓",
        TicketStatus::InProgress => "▶",
        TicketStatus::New => "○",
        TicketStatus::Cancelled => "✗",
        TicketStatus::Next => "◎",
        TicketStatus::Archived => "▣",
    }
}

// ============================================================================
// Ticket Row Component
// ============================================================================

#[derive(Default, Props)]
pub struct TicketRowProps {
    pub ticket: Option<HudTicket>,
    pub is_selected: bool,
    pub flash: Option<FlashType>,
    pub width: u32,
    /// Pre-computed layout for column alignment (None falls back to simple layout)
    pub layout: Option<PhaseLayout>,
}

#[component]
pub fn TicketRow(props: &TicketRowProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();

    let Some(ticket) = &props.ticket else {
        return element! {
            View(height: 1)
        };
    };

    let status = ticket
        .metadata
        .as_ref()
        .and_then(|m| m.status)
        .unwrap_or(TicketStatus::New);

    let title = ticket
        .metadata
        .as_ref()
        .and_then(|m| m.title.as_deref())
        .unwrap_or("[missing]");

    let icon = status_icon(status);
    let icon_color = theme.status_color(status);
    let is_complete = status == TicketStatus::Complete;
    let is_cancelled = status == TicketStatus::Cancelled;
    let is_dimmed = is_complete || is_cancelled;

    // Determine text color based on state
    let text_color = if props.flash == Some(FlashType::Completed) {
        theme.status_complete
    } else if props.flash == Some(FlashType::Started) {
        theme.status_in_progress
    } else if is_dimmed {
        theme.text_dimmed
    } else {
        theme.text
    };

    let id_color = if is_dimmed {
        theme.text_dimmed
    } else {
        theme.id_color
    };

    let bg_color = if props.is_selected {
        theme.highlight
    } else {
        Color::Reset
    };

    // Build the right-column text (annotation or duration)
    let right_text = ticket_right_text(ticket);
    let right_color = if ticket.is_active {
        theme.status_in_progress
    } else {
        theme.text_dimmed
    };
    let right_weight = if ticket.is_active {
        Weight::Bold
    } else {
        Weight::Normal
    };

    if let Some(ref layout) = props.layout {
        // Aligned layout: fixed columns with title wrapping
        let id_padded = format!("{:width$}", ticket.id, width = layout.id_col_width);

        // Right column: pad to fixed width, right-aligned
        let right_padded = format!("{:>width$}", right_text, width = layout.right_col_width);

        // Split title into lines that fit within title_max_width
        let title_lines = wrap_text(title, layout.title_max_width);
        let continuation_pad = " ".repeat(layout.continuation_indent);

        // Build elements for each line
        let mut row_elements: Vec<AnyElement<'static>> = Vec::new();

        // First line: icon + id + title_line_1 + right_col
        let first_title = title_lines.first().cloned().unwrap_or_default();
        // Pad the first title line to fill the title column so right-col aligns
        let first_title_padded = format!("{:width$}", first_title, width = layout.title_max_width);

        row_elements.push(
            element! {
                View(
                    height: 1,
                    width: 100pct,
                    flex_direction: FlexDirection::Row,
                    background_color: bg_color,
                    padding_left: 2,
                ) {
                    Text(content: format!("{icon} "), color: icon_color)
                    Text(content: id_padded.clone(), color: id_color)
                    Text(content: first_title_padded, color: text_color)
                    Text(content: format!(" {right_padded}"), color: right_color, weight: right_weight)
                }
            }
            .into(),
        );

        // Continuation lines (indented, no icon/id/right-col)
        for line in title_lines.iter().skip(1) {
            let cont_line = continuation_pad.clone() + line;
            row_elements.push(
                element! {
                    View(
                        height: 1,
                        width: 100pct,
                        background_color: bg_color,
                    ) {
                        Text(content: cont_line, color: text_color)
                    }
                }
                .into(),
            );
        }

        element! {
            View(
                width: 100pct,
                flex_direction: FlexDirection::Column,
            ) {
                #(row_elements)
            }
        }
    } else {
        // Fallback: simple single-line layout (no alignment)
        let max_title_len = props.width.saturating_sub(20).min(80) as usize;
        let display_title = if title.len() > max_title_len && max_title_len > 3 {
            format!("{}...", &title[..max_title_len - 3])
        } else {
            title.to_string()
        };

        element! {
            View(
                height: 1,
                width: 100pct,
                flex_direction: FlexDirection::Row,
                background_color: bg_color,
                padding_left: 2,
            ) {
                Text(content: format!("{icon} "), color: icon_color)
                Text(content: format!("{} ", ticket.id), color: id_color)
                Text(content: display_title, color: text_color)
                #(if !right_text.is_empty() {
                    Some(element! {
                        Text(content: format!(" {right_text}"), color: right_color, weight: right_weight)
                    })
                } else {
                    None
                })
            }
        }
    }
}

/// Wrap text into lines of at most `max_width` characters.
/// Tries to break at word boundaries.
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    if text.len() <= max_width {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_width {
            lines.push(remaining.to_string());
            break;
        }

        // Find a good break point (last space within max_width)
        let break_at = remaining[..max_width].rfind(' ').unwrap_or(max_width); // hard break if no space

        let (line, rest) = remaining.split_at(break_at);
        lines.push(line.to_string());
        remaining = rest.trim_start();
    }

    if lines.is_empty() {
        lines.push(text.to_string());
    }
    lines
}

// ============================================================================
// Phase Header Component
// ============================================================================

#[derive(Default, Props)]
pub struct PhaseHeaderProps {
    pub number: String,
    pub name: String,
    pub status: Option<TicketStatus>,
    pub completed: usize,
    pub total: usize,
    pub is_active_phase: bool,
    pub flash: Option<FlashType>,
    pub width: u32,
    /// Column where the progress bar should start (for global alignment).
    /// If None, the bar follows immediately after the name.
    pub bar_start_col: Option<usize>,
}

#[component]
pub fn PhaseHeader(props: &PhaseHeaderProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();
    let status = props.status.unwrap_or(TicketStatus::New);
    let icon = phase_status_icon(status);
    let is_complete = status == TicketStatus::Complete;

    let name_color = if props.flash == Some(FlashType::PhaseCompleted) {
        theme.status_complete
    } else if is_complete {
        theme.text_dimmed
    } else if props.is_active_phase {
        theme.text
    } else {
        theme.text_dimmed
    };

    let icon_color = if props.flash == Some(FlashType::PhaseCompleted) {
        theme.status_complete
    } else {
        theme.status_color(status)
    };

    let weight = if props.is_active_phase || props.flash == Some(FlashType::PhaseCompleted) {
        Weight::Bold
    } else {
        Weight::Normal
    };

    // Phase label text
    let label = format!("Phase {}: {}", props.number, props.name);

    // Compute the label column width: either from bar_start_col or auto
    let label_col_width = props
        .bar_start_col
        .map(|col| col.saturating_sub(ICON_WIDTH))
        .unwrap_or(label.len() + 1);

    // Pad or truncate the label to fit the column
    let display_label = if label.len() >= label_col_width {
        // Truncate with ellipsis if too long
        if label_col_width > 3 {
            format!(
                "{:.<width$}",
                &label[..label_col_width - 1],
                width = label_col_width
            )
        } else {
            label[..label_col_width].to_string()
        }
    } else {
        format!("{:width$}", label, width = label_col_width)
    };

    // Progress bar: fill remaining space
    let bar_and_count_width = props.width as usize;
    let used = ICON_WIDTH + label_col_width + 1; // +1 for spacing before count
    let bar_width = bar_and_count_width.saturating_sub(used + 6).clamp(4, 18);
    let (bar_filled, bar_empty) =
        render_progress_bar_parts(props.completed, props.total, bar_width);
    // Completed portions are always green; incomplete portions use teal for the active phase, grey otherwise
    let filled_color = theme.status_complete;
    let empty_color = if props.is_active_phase {
        theme.status_in_progress
    } else {
        theme.text_dimmed
    };

    let progress_text = format!("{}/{}", props.completed, props.total);

    element! {
        View(
            height: 1,
            width: 100pct,
            flex_direction: FlexDirection::Row,
            margin_top: 1,
        ) {
            Text(
                content: format!("{icon} "),
                color: icon_color,
                weight: weight,
            )
            Text(
                content: display_label,
                color: name_color,
                weight: weight,
            )
            Text(content: bar_filled, color: filled_color)
            Text(content: bar_empty, color: empty_color)
            Text(content: format!(" {progress_text}"), color: theme.text_dimmed)
        }
    }
}

// ============================================================================
// Activity Log Component
// ============================================================================

#[derive(Default, Props)]
pub struct ActivityLogProps {
    pub events: Vec<ActivityEvent>,
    pub height: u32,
    pub width: u32,
}

#[component]
pub fn ActivityLog(props: &ActivityLogProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();
    let visible_count = props.height.saturating_sub(2) as usize; // border takes 2 lines

    element! {
        View(
            width: 100pct,
            height: props.height,
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Single,
            border_color: theme.border,
            border_edges: Edges::Top,
            overflow: Overflow::Hidden,
        ) {
            // Header
            View(height: 1, padding_left: 1) {
                Text(content: "Activity", color: theme.text_dimmed, weight: Weight::Bold)
            }
            // Event rows
            #(props.events.iter().take(visible_count).map(|event| {
                let icon = match event.event_type {
                    crate::events::types::EventType::StatusChanged => {
                        if event.description.contains("complete") {
                            "✓"
                        } else if event.description.contains("in_progress") {
                            "●"
                        } else {
                            "◦"
                        }
                    }
                    _ => "◦",
                };

                let icon_color = match event.event_type {
                    crate::events::types::EventType::StatusChanged => {
                        if event.description.contains("complete") {
                            theme.status_complete
                        } else if event.description.contains("in_progress") {
                            theme.status_in_progress
                        } else {
                            theme.text_dimmed
                        }
                    }
                    _ => theme.text_dimmed,
                };

                element! {
                    View(height: 1, width: 100pct, flex_direction: FlexDirection::Row, padding_left: 1) {
                        Text(content: format!("{} ", event.time), color: theme.text_dimmed)
                        Text(content: format!("{icon} "), color: icon_color)
                        Text(content: format!("{} ", event.entity_id), color: theme.id_color)
                        Text(content: event.description.clone(), color: theme.text_dimmed)
                    }
                }
            }))
        }
    }
}

// ============================================================================
// Plan Complete Banner
// ============================================================================

#[derive(Default, Props)]
pub struct PlanCompleteBannerProps {
    pub plan_title: String,
    pub total_tickets: usize,
    pub elapsed: Option<String>,
}

#[component]
pub fn PlanCompleteBanner(props: &PlanCompleteBannerProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();

    let subtitle = if let Some(ref elapsed) = props.elapsed {
        format!("{} tickets completed in {}", props.total_tickets, elapsed)
    } else {
        format!("{} tickets completed", props.total_tickets)
    };

    element! {
        View(
            width: 100pct,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            margin_top: 2,
            margin_bottom: 2,
        ) {
            View(
                height: 4,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border_style: BorderStyle::Double,
                border_color: theme.status_complete,
                padding_left: 4,
                padding_right: 4,
            ) {
                Text(
                    content: "PLAN COMPLETE",
                    color: theme.status_complete,
                    weight: Weight::Bold,
                )
                Text(
                    content: subtitle,
                    color: theme.text_dimmed,
                )
            }
        }
    }
}

// ============================================================================
// Detail Panel (right pane in wide mode)
// ============================================================================

#[derive(Default, Props)]
pub struct DetailPanelProps {
    /// The ticket to show in the detail pane
    pub ticket: Option<crate::types::TicketMetadata>,
    /// Pre-loaded body content for the ticket
    pub body: String,
    /// Activity events for the bottom section
    pub events: Vec<ActivityEvent>,
    /// Active ticket timing string (e.g., "Active for: 8m")
    pub active_time_str: Option<String>,
}

/// Right-side panel showing ticket detail (top) + activity log (bottom)
#[component]
pub fn DetailPanel(props: &DetailPanelProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();

    element! {
        View(
            width: 100pct,
            height: 100pct,
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Round,
            border_color: theme.border,
            overflow: Overflow::Hidden,
        ) {
            // Top section: Ticket detail (~65%)
            View(
                width: 100pct,
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                overflow: Overflow::Hidden,
            ) {
                #(if let Some(ref ticket) = props.ticket {
                    let id = ticket.id.as_ref().map(|id| id.to_string()).unwrap_or_else(|| "???".to_string());
                    let title = ticket.title.clone().unwrap_or_else(|| "(no title)".to_string());
                    let status = ticket.status.unwrap_or_default();
                    let ticket_type = ticket.ticket_type;
                    let priority = ticket.priority;
                    let size = ticket.size;

                    let status_color = theme.status_color(status);
                    let type_color = ticket_type.map(|t| theme.type_color(t)).unwrap_or(theme.text);
                    let priority_color = priority.map(|p| theme.priority_color(p)).unwrap_or(theme.text);

                    let status_str = status.to_string();
                    let type_str = ticket_type.map(|t| t.to_string()).unwrap_or_else(|| "-".to_string());
                    let priority_str = priority.map(|p| format!("P{}", p.as_num())).unwrap_or_else(|| "-".to_string());
                    let size_str = size.map(|s| s.to_string()).unwrap_or_else(|| "-".to_string());

                    Some(element! {
                        View(
                            width: 100pct,
                            flex_grow: 1.0,
                            flex_direction: FlexDirection::Column,
                            overflow: Overflow::Hidden,
                        ) {
                            // Header: ID + Title
                            View(
                                width: 100pct,
                                padding: 1,
                                flex_shrink: 0.0,
                                border_edges: Edges::Bottom,
                                border_style: BorderStyle::Single,
                                border_color: theme.border,
                                flex_direction: FlexDirection::Column,
                            ) {
                                Text(content: id, color: theme.id_color, weight: Weight::Bold)
                                Text(content: title, color: theme.text, weight: Weight::Bold)
                            }

                            // Compact metadata
                            View(
                                width: 100pct,
                                padding_left: 1,
                                padding_right: 1,
                                padding_top: 1,
                                flex_shrink: 0.0,
                                flex_direction: FlexDirection::Column,
                            ) {
                                View(flex_direction: FlexDirection::Row, height: 1) {
                                    Text(content: "Status: ", color: theme.text_dimmed)
                                    Text(content: format!("{status_str}  "), color: status_color)
                                    Text(content: "Type: ", color: theme.text_dimmed)
                                    Text(content: type_str, color: type_color)
                                }
                                View(flex_direction: FlexDirection::Row, height: 1) {
                                    Text(content: "Priority: ", color: theme.text_dimmed)
                                    Text(content: format!("{priority_str}  "), color: priority_color)
                                    Text(content: "Size: ", color: theme.text_dimmed)
                                    Text(content: size_str, color: theme.text)
                                }
                                #(props.active_time_str.as_ref().map(|time| element! {
                                    View(flex_direction: FlexDirection::Row, height: 1) {
                                        Text(content: "Active for: ", color: theme.text_dimmed)
                                        Text(content: time.clone(), color: theme.status_in_progress)
                                    }
                                }))
                            }

                            // Body
                            View(
                                flex_grow: 1.0,
                                width: 100pct,
                                padding: 1,
                                overflow: Overflow::Hidden,
                                border_edges: Edges::Top,
                                border_style: BorderStyle::Single,
                                border_color: theme.border,
                                margin_top: 1,
                            ) {
                                crate::tui::components::TextViewer(
                                    text: props.body.clone(),
                                    scroll_offset: 0usize,
                                    has_focus: false,
                                    placeholder: None,
                                    markdown: true,
                                )
                            }
                        }
                    })
                } else {
                    Some(element! {
                        View(
                            width: 100pct,
                            flex_grow: 1.0,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                        ) {
                            Text(content: "No active ticket", color: theme.text_dimmed)
                        }
                    })
                })
            }

            // Bottom section: Activity log
            #(if !props.events.is_empty() {
                let visible_count = 8usize;
                Some(element! {
                    View(
                        width: 100pct,
                        flex_shrink: 0.0,
                        flex_direction: FlexDirection::Column,
                        border_edges: Edges::Top,
                        border_style: BorderStyle::Single,
                        border_color: theme.border,
                        overflow: Overflow::Hidden,
                    ) {
                        // Activity header
                        View(height: 1, padding_left: 1) {
                            Text(content: "Activity", color: theme.text_dimmed, weight: Weight::Bold)
                        }
                        // Event rows
                        #(props.events.iter().take(visible_count).map(|event| {
                            let icon = match event.event_type {
                                crate::events::types::EventType::StatusChanged => {
                                    if event.description.contains("complete") {
                                        "✓"
                                    } else if event.description.contains("in_progress") {
                                        "●"
                                    } else {
                                        "◦"
                                    }
                                }
                                _ => "◦",
                            };
                            let icon_color = match event.event_type {
                                crate::events::types::EventType::StatusChanged => {
                                    if event.description.contains("complete") {
                                        theme.status_complete
                                    } else if event.description.contains("in_progress") {
                                        theme.status_in_progress
                                    } else {
                                        theme.text_dimmed
                                    }
                                }
                                _ => theme.text_dimmed,
                            };
                            element! {
                                View(height: 1, width: 100pct, flex_direction: FlexDirection::Row, padding_left: 1) {
                                    Text(content: format!("{} ", event.time), color: theme.text_dimmed)
                                    Text(content: format!("{icon} "), color: icon_color)
                                    Text(content: format!("{} ", event.entity_id), color: theme.id_color)
                                    Text(content: event.description.clone(), color: theme.text_dimmed)
                                }
                            }
                        }))
                    }
                })
            } else {
                None
            })
        }
    }
}

// ============================================================================
// Scroll Indicator
// ============================================================================

#[derive(Default, Props)]
pub struct ScrollIndicatorProps {
    /// Show "more above" arrow
    pub has_more_above: bool,
    /// Show "more below" arrow
    pub has_more_below: bool,
    /// Which direction this indicator is for: true = above, false = below
    pub is_above: bool,
    pub width: u32,
}

/// A single-line scroll indicator (▲ or ▼) shown at the top/bottom of scrollable content
#[component]
pub fn ScrollIndicator(props: &ScrollIndicatorProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();
    let show = if props.is_above {
        props.has_more_above
    } else {
        props.has_more_below
    };

    if show {
        let arrow = if props.is_above { "▲" } else { "▼" };
        element! {
            View(
                height: 1,
                width: 100pct,
                justify_content: JustifyContent::Center,
            ) {
                Text(content: arrow.to_string(), color: theme.text_dimmed)
            }
        }
    } else {
        element! {
            View(height: 0)
        }
    }
}

// ============================================================================
// Helper: Check flash state
// ============================================================================

/// Check if a ticket/phase has an active flash
pub fn get_flash(flashes: &HashMap<String, FlashEntry>, key: &str) -> Option<FlashType> {
    flashes.get(key).and_then(|entry| {
        if entry.created.elapsed().as_secs() < FLASH_DURATION_SECS {
            Some(entry.flash_type)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn test_render_progress_bar_parts() {
        let (filled, empty) = render_progress_bar_parts(5, 10, 10);
        assert_eq!(filled, "░░░░░");
        assert_eq!(empty, "░░░░░");

        let (filled, empty) = render_progress_bar_parts(0, 10, 10);
        assert_eq!(filled, "");
        assert_eq!(empty, "░░░░░░░░░░");

        let (filled, empty) = render_progress_bar_parts(10, 10, 10);
        assert_eq!(filled, "░░░░░░░░░░");
        assert_eq!(empty, "");

        let (filled, empty) = render_progress_bar_parts(0, 0, 10);
        assert_eq!(filled, "");
        assert_eq!(empty, "░░░░░░░░░░");
    }

    #[test]
    fn test_render_percent() {
        assert_eq!(render_percent(5, 10), "50%");
        assert_eq!(render_percent(0, 10), "0%");
        assert_eq!(render_percent(10, 10), "100%");
        assert_eq!(render_percent(0, 0), "0%");
    }

    #[test]
    fn test_status_icon() {
        assert_eq!(status_icon(TicketStatus::Complete), "✓");
        assert_eq!(status_icon(TicketStatus::InProgress), "●");
        assert_eq!(status_icon(TicketStatus::New), "○");
        assert_eq!(status_icon(TicketStatus::Next), "◎");
        assert_eq!(status_icon(TicketStatus::Cancelled), "✗");
    }

    #[test]
    fn test_get_flash_active() {
        let mut flashes = HashMap::new();
        flashes.insert(
            "j-a1b2".to_string(),
            FlashEntry {
                flash_type: FlashType::Completed,
                created: Instant::now(),
            },
        );
        assert_eq!(get_flash(&flashes, "j-a1b2"), Some(FlashType::Completed));
        assert_eq!(get_flash(&flashes, "j-xxxx"), None);
    }
}
