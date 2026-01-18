//! Remote TUI list pane component
//!
//! Displays either the local tickets list or the remote issues list
//! with selection state, markers, and link indicators.

use std::collections::HashSet;

use iocraft::prelude::*;

use crate::remote::RemoteStatus;
use crate::tui::remote::filter::{FilteredLocalTicket, FilteredRemoteIssue};
use crate::tui::remote::state::ViewMode;
use crate::tui::theme::theme;
use crate::types::TicketMetadata;

/// Props for the ListPane component
#[derive(Default, Props)]
pub struct ListPaneProps {
    /// Current view mode (Local or Remote)
    pub view_mode: ViewMode,
    /// Whether remote issues are currently loading
    pub is_loading: bool,
    /// Filtered local tickets to display (already paginated)
    pub local_list: Vec<FilteredLocalTicket>,
    /// Filtered remote issues to display (already paginated)
    pub remote_list: Vec<FilteredRemoteIssue>,
    /// Total count of filtered local tickets
    pub local_count: usize,
    /// Total count of filtered remote issues
    pub remote_count: usize,
    /// Current scroll offset for local list
    pub local_scroll_offset: usize,
    /// Current scroll offset for remote list
    pub remote_scroll_offset: usize,
    /// Currently selected index in local list
    pub local_selected_index: usize,
    /// Currently selected index in remote list
    pub remote_selected_index: usize,
    /// Set of selected local ticket IDs (for multi-select)
    pub local_selected_ids: HashSet<String>,
    /// Set of selected remote issue IDs (for multi-select)
    pub remote_selected_ids: HashSet<String>,
    /// All local tickets (for checking link status of remote issues)
    pub all_local_tickets: Vec<TicketMetadata>,
}

/// List pane showing either local tickets or remote issues
#[component]
pub fn ListPane(props: &ListPaneProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();

    element! {
        View(
            width: 40pct,
            height: 100pct,
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Round,
            border_color: theme.border_focused,
        ) {
            #(if props.view_mode == ViewMode::Remote {
                render_remote_list(props)
            } else {
                render_local_list(props)
            })
        }
    }
}

/// Render the remote issues list
fn render_remote_list(props: &ListPaneProps) -> Option<AnyElement<'static>> {
    let theme = theme();

    if props.is_loading {
        return Some(
            element! {
                View(
                    flex_grow: 1.0,
                    width: 100pct,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                ) {
                    Text(content: "Loading remote issues...", color: theme.text_dimmed)
                }
            }
            .into_any(),
        );
    }

    if props.remote_count == 0 {
        return Some(
            element! {
                View(
                    flex_grow: 1.0,
                    width: 100pct,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                ) {
                    Text(content: "No remote issues found", color: theme.text_dimmed)
                }
            }
            .into_any(),
        );
    }

    // Clone data for rendering
    let remote_list = props.remote_list.clone();
    let remote_scroll_offset = props.remote_scroll_offset;
    let remote_selected_index = props.remote_selected_index;
    let remote_selected_ids = props.remote_selected_ids.clone();
    let all_local_tickets = props.all_local_tickets.clone();

    Some(
        element! {
            View(
                width: 100pct,
                height: 100pct,
                flex_direction: FlexDirection::Column,
            ) {
                #(remote_list.iter().enumerate().map(|(i, filtered)| {
                    let actual_idx = remote_scroll_offset + i;
                    let is_selected = actual_idx == remote_selected_index;
                    let issue = &filtered.issue;
                    let is_marked = remote_selected_ids.contains(&issue.id);

                    let status_color = match &issue.status {
                        RemoteStatus::Open => Color::Green,
                        RemoteStatus::Closed => Color::DarkGrey,
                        RemoteStatus::Custom(_) => Color::White,
                    };

                    let indicator = if is_selected { ">" } else { " " };
                    let marker = if is_marked { "*" } else { " " };
                    let is_linked = all_local_tickets.iter().any(|t| {
                        t.remote.as_ref().is_some_and(|r| r.contains(&issue.id))
                    });
                    let link_indicator = if is_linked { "⟷" } else { " " };

                    let status_str = match &issue.status {
                        RemoteStatus::Open => "open".to_string(),
                        RemoteStatus::Closed => "closed".to_string(),
                        RemoteStatus::Custom(s) => s.clone(),
                    };

                    let title_display = if issue.title.len() > 25 {
                        format!("{}...", &issue.title[..22])
                    } else {
                        issue.title.clone()
                    };

                    element! {
                        View(
                            height: 1,
                            width: 100pct,
                            padding_left: 1,
                            background_color: if is_selected { Some(theme.highlight) } else { None },
                        ) {
                            Text(content: indicator.to_string(), color: Color::White)
                            Text(content: marker.to_string(), color: Color::White)
                            Text(content: link_indicator.to_string(), color: Color::Cyan)
                            Text(
                                content: format!(" {:<10}", &issue.id),
                                color: if is_selected { Color::White } else { theme.id_color },
                            )
                            Text(
                                content: format!(" [{}]", status_str),
                                color: if is_selected { Color::White } else { status_color },
                            )
                            Text(
                                content: format!(" {}", title_display),
                                color: Color::White,
                            )
                        }
                    }
                }))
            }
        }
        .into_any(),
    )
}

/// Render the local tickets list
fn render_local_list(props: &ListPaneProps) -> Option<AnyElement<'static>> {
    let theme = theme();

    if props.local_count == 0 {
        return Some(
            element! {
                View(
                    flex_grow: 1.0,
                    width: 100pct,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                ) {
                    Text(content: "No local tickets", color: theme.text_dimmed)
                }
            }
            .into_any(),
        );
    }

    // Clone data for rendering
    let local_list = props.local_list.clone();
    let local_scroll_offset = props.local_scroll_offset;
    let local_selected_index = props.local_selected_index;
    let local_selected_ids = props.local_selected_ids.clone();

    Some(
        element! {
            View(
                width: 100pct,
                height: 100pct,
                flex_direction: FlexDirection::Column,
            ) {
                #(local_list.iter().enumerate().map(|(i, filtered)| {
                    let actual_idx = local_scroll_offset + i;
                    let is_selected = actual_idx == local_selected_index;
                    let ticket = &filtered.ticket;
                    let ticket_id = ticket.id.as_deref().unwrap_or("???");
                    let is_marked = local_selected_ids.contains(ticket_id);

                    let status = ticket.status.unwrap_or_default();
                    let status_color = theme.status_color(status);

                    let indicator = if is_selected { ">" } else { " " };
                    let marker = if is_marked { "*" } else { " " };
                    let link_indicator = if ticket.remote.is_some() { "⟷" } else { " " };

                    let title = ticket.title.as_deref().unwrap_or("(no title)");
                    let title_display = if title.len() > 25 {
                        format!("{}...", &title[..22])
                    } else {
                        title.to_string()
                    };

                    let status_str = match status {
                        crate::types::TicketStatus::New => "new",
                        crate::types::TicketStatus::Next => "nxt",
                        crate::types::TicketStatus::InProgress => "wip",
                        crate::types::TicketStatus::Complete => "don",
                        crate::types::TicketStatus::Cancelled => "can",
                    };

                    element! {
                        View(
                            height: 1,
                            width: 100pct,
                            padding_left: 1,
                            background_color: if is_selected { Some(theme.highlight) } else { None },
                        ) {
                            Text(content: indicator.to_string(), color: Color::White)
                            Text(content: marker.to_string(), color: Color::White)
                            Text(content: link_indicator.to_string(), color: Color::Cyan)
                            Text(
                                content: format!(" {:<8}", ticket_id),
                                color: if is_selected { Color::White } else { theme.id_color },
                            )
                            Text(
                                content: format!(" [{}]", status_str),
                                color: if is_selected { Color::White } else { status_color },
                            )
                            Text(
                                content: format!(" {}", title_display),
                                color: Color::White,
                            )
                        }
                    }
                }))
            }
        }
        .into_any(),
    )
}
