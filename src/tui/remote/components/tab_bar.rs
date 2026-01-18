//! Remote TUI tab bar component
//!
//! Displays the [Local] [Remote] toggle tabs with optional filter query display.

use iocraft::prelude::*;

use crate::tui::remote::state::ViewMode;
use crate::tui::theme::theme;

/// Props for the TabBar component
#[derive(Default, Props)]
pub struct TabBarProps {
    /// The currently active view mode
    pub active_view: ViewMode,
    /// Optional filter query to display
    pub filter_query: Option<String>,
}

/// Tab bar showing [Local] [Remote] toggle with optional filter display
#[component]
pub fn TabBar(props: &TabBarProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();
    let current_view = props.active_view;
    let query = props.filter_query.clone().unwrap_or_default();

    element! {
        View(
            width: 100pct,
            padding_left: 1,
            border_edges: Edges::Bottom,
            border_style: BorderStyle::Single,
            border_color: theme.border,
        ) {
            Text(
                content: "[Local] ",
                color: if current_view == ViewMode::Local { Color::Cyan } else { theme.text_dimmed },
                weight: if current_view == ViewMode::Local { Weight::Bold } else { Weight::Normal },
            )
            Text(
                content: "[Remote] ",
                color: if current_view == ViewMode::Remote { Color::Cyan } else { theme.text_dimmed },
                weight: if current_view == ViewMode::Remote { Weight::Bold } else { Weight::Normal },
            )
            View(flex_grow: 1.0)
            #(if query.is_empty() {
                None
            } else {
                Some(element! {
                    Text(
                        content: format!(" Filter: {}", query),
                        color: Color::Yellow,
                    )
                })
            })
        }
    }
}
