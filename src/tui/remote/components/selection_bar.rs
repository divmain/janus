//! Remote TUI selection status bar component
//!
//! Displays the "X selected" bar when items are selected.

use iocraft::prelude::*;

use crate::tui::remote::state::ViewMode;
use crate::tui::theme::theme;

/// Props for the SelectionBar component
#[derive(Default, Props)]
pub struct SelectionBarProps {
    /// Current view mode (Local or Remote)
    pub view_mode: ViewMode,
    /// Number of selected local tickets
    pub local_count: usize,
    /// Number of selected remote issues
    pub remote_count: usize,
    /// Status message to display (e.g., "Showing X issues", "Found Y matches")
    pub status_message: Option<String>,
}

/// Selection status bar showing "X selected" and optional status message
#[component]
pub fn SelectionBar(props: &SelectionBarProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();

    let count = if props.view_mode == ViewMode::Remote {
        props.remote_count
    } else {
        props.local_count
    };

    // Build the content to display
    let has_selection = count > 0;
    let has_status = props.status_message.is_some();

    if !has_selection && !has_status {
        return element! {
            View()
        };
    }

    let content = if has_selection && has_status {
        format!(
            "{} | {count} selected",
            props.status_message.as_ref().unwrap(),
        )
    } else if has_selection {
        format!("{count} selected")
    } else {
        props.status_message.as_ref().unwrap().clone()
    };

    element! {
        View(
            width: 100pct,
            flex_shrink: 0.0,
            padding_left: 1,
            border_edges: Edges::Top,
            border_style: BorderStyle::Single,
            border_color: theme.border,
        ) {
            Text(content: content, color: if has_selection { Color::Cyan } else { theme.text_dimmed })
        }
    }
}
