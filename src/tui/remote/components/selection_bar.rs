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
}

/// Selection status bar showing "X selected"
#[component]
pub fn SelectionBar(props: &SelectionBarProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();

    let count = if props.view_mode == ViewMode::Remote {
        props.remote_count
    } else {
        props.local_count
    };

    if count == 0 {
        return element! {
            View()
        };
    }

    element! {
        View(
            width: 100pct,
            flex_shrink: 0.0,
            padding_left: 1,
            border_edges: Edges::Top,
            border_style: BorderStyle::Single,
            border_color: theme.border,
        ) {
            Text(content: format!("{} selected", count), color: Color::Cyan)
        }
    }
}
