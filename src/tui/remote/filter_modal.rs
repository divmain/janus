//! Filter modal for remote TUI
//!
//! Provides advanced filtering of remote issues by status, assignee, labels, etc.

use iocraft::prelude::*;

use crate::remote::{RemoteQuery, RemoteStatusFilter};

/// Filter modal state
#[derive(Debug, Clone, Default)]
pub struct FilterState {
    /// Current status filter
    pub status: Option<RemoteStatusFilter>,
    /// Assignee filter
    pub assignee: String,
    /// Labels filter (comma-separated)
    pub labels: String,
    /// Current focused field index
    pub focused_field: usize,
}

impl FilterState {
    /// Create a new filter state from an existing query
    pub fn from_query(query: &RemoteQuery) -> Self {
        Self {
            status: query.status,
            assignee: query.assignee.clone().unwrap_or_default(),
            labels: query
                .labels
                .as_ref()
                .map(|l| l.join(", "))
                .unwrap_or_default(),
            focused_field: 0,
        }
    }

    /// Convert to a RemoteQuery
    pub fn to_query(&self, base: &RemoteQuery) -> RemoteQuery {
        let mut query = base.clone();
        query.status = self.status;
        query.assignee = if self.assignee.is_empty() {
            None
        } else {
            Some(self.assignee.clone())
        };
        query.labels = if self.labels.is_empty() {
            None
        } else {
            Some(
                self.labels
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            )
        };
        query
    }

    /// Number of filterable fields
    pub const FIELD_COUNT: usize = 3;

    /// Move focus to next field
    pub fn focus_next(&mut self) {
        self.focused_field = (self.focused_field + 1) % Self::FIELD_COUNT;
    }

    /// Move focus to previous field
    pub fn focus_prev(&mut self) {
        if self.focused_field == 0 {
            self.focused_field = Self::FIELD_COUNT - 1;
        } else {
            self.focused_field -= 1;
        }
    }

    /// Toggle status filter value
    pub fn toggle_status(&mut self) {
        self.status = match self.status {
            None => Some(RemoteStatusFilter::Open),
            Some(RemoteStatusFilter::Open) => Some(RemoteStatusFilter::Closed),
            Some(RemoteStatusFilter::Closed) => Some(RemoteStatusFilter::All),
            Some(RemoteStatusFilter::All) => None,
        };
    }

    /// Check if any filter is active
    pub fn has_active_filters(&self) -> bool {
        self.status.is_some() || !self.assignee.is_empty() || !self.labels.is_empty()
    }

    /// Clear all filters
    pub fn clear(&mut self) {
        self.status = None;
        self.assignee.clear();
        self.labels.clear();
    }
}

/// Props for the filter modal
#[derive(Default, Props)]
pub struct FilterModalProps {
    pub state: FilterState,
}

/// Filter modal component
#[component]
pub fn FilterModal<'a>(props: &FilterModalProps, _hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let state = &props.state;

    let status_str = match state.status {
        None => "All",
        Some(RemoteStatusFilter::Open) => "Open",
        Some(RemoteStatusFilter::Closed) => "Closed",
        Some(RemoteStatusFilter::All) => "All",
    };

    element! {
        View(
            width: 100pct,
            height: 100pct,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            background_color: Color::Black,
        ) {
            View(
                width: 60,
                height: 16,
                border_style: BorderStyle::Double,
                border_color: Color::Cyan,
                padding: 1,
                flex_direction: FlexDirection::Column,
                background_color: Color::Rgb { r: 120, g: 120, b: 120 },
            ) {
                // Header
                Text(
                    content: "Filter Remote Issues",
                    color: Color::Cyan,
                    weight: Weight::Bold,
                )
                Text(content: "")

                // Status field
                View(
                    width: 100pct,
                    flex_direction: FlexDirection::Row,
                    background_color: if state.focused_field == 0 { Some(Color::DarkBlue) } else { None },
                ) {
                    Text(
                        content: "Status: ",
                        color: if state.focused_field == 0 { Color::Yellow } else { Color::White },
                    )
                    Text(
                        content: format!("[{}]", status_str),
                        color: Color::Cyan,
                    )
                    Text(
                        content: if state.focused_field == 0 { " (Enter to toggle)" } else { "" },
                        color: Color::Rgb { r: 120, g: 120, b: 120 },
                    )
                }
                Text(content: "")

                // Assignee field
                View(
                    width: 100pct,
                    flex_direction: FlexDirection::Row,
                    background_color: if state.focused_field == 1 { Some(Color::DarkBlue) } else { None },
                ) {
                    Text(
                        content: "Assignee: ",
                        color: if state.focused_field == 1 { Color::Yellow } else { Color::White },
                    )
                    Text(
                        content: if state.assignee.is_empty() { "(any)" } else { &state.assignee },
                        color: if state.assignee.is_empty() { Color::Rgb { r: 120, g: 120, b: 120 } } else { Color::Cyan },
                    )
                }
                Text(content: "")

                // Labels field
                View(
                    width: 100pct,
                    flex_direction: FlexDirection::Row,
                    background_color: if state.focused_field == 2 { Some(Color::DarkBlue) } else { None },
                ) {
                    Text(
                        content: "Labels: ",
                        color: if state.focused_field == 2 { Color::Yellow } else { Color::White },
                    )
                    Text(
                        content: if state.labels.is_empty() { "(any)" } else { &state.labels },
                        color: if state.labels.is_empty() { Color::Rgb { r: 120, g: 120, b: 120 } } else { Color::Cyan },
                    )
                }
                Text(content: "")

                // Divider
                View(
                    width: 100pct,
                    border_edges: Edges::Bottom,
                    border_style: BorderStyle::Single,
                    border_color: Color::Rgb { r: 120, g: 120, b: 120 },
                ) {
                    Text(content: "")
                }
                Text(content: "")

                // Help text
                Text(
                    content: "Tab/j/k: navigate | Enter: toggle/edit | x: clear | Esc: cancel | Enter: apply",
                    color: Color::Rgb { r: 120, g: 120, b: 120 },
                )
            }
        }
    }
}
