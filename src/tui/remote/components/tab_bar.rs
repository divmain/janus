//! Remote TUI tab bar component
//!
//! Displays the [Local] [Remote] toggle tabs with optional filter query display.

use iocraft::prelude::*;

use crate::tui::components::ClickableText;
use crate::tui::remote::state::ViewMode;
use crate::tui::theme::theme;

/// Props for the TabBar component
#[derive(Default, Props)]
pub struct TabBarProps {
    /// The currently active view mode
    pub active_view: ViewMode,
    /// Optional filter query to display
    pub filter_query: Option<String>,
    /// Handler invoked when Local tab is clicked
    pub on_local_click: Option<Handler<()>>,
    /// Handler invoked when Remote tab is clicked
    pub on_remote_click: Option<Handler<()>>,
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
            flex_shrink: 0.0,
            padding_left: 1,
            border_edges: Edges::Bottom,
            border_style: BorderStyle::Single,
            border_color: theme.border,
        ) {
            ClickableText(
                content: "[Local] ".to_string(),
                on_click: props.on_local_click.clone(),
                color: if current_view == ViewMode::Local { Some(Color::Cyan) } else { Some(theme.text_dimmed) },
                hover_color: Some(Color::Cyan),
                weight: if current_view == ViewMode::Local { Some(Weight::Bold) } else { Some(Weight::Normal) },
                hover_weight: Some(Weight::Bold),
            )
            ClickableText(
                content: "[Remote] ".to_string(),
                on_click: props.on_remote_click.clone(),
                color: if current_view == ViewMode::Remote { Some(Color::Cyan) } else { Some(theme.text_dimmed) },
                hover_color: Some(Color::Cyan),
                weight: if current_view == ViewMode::Remote { Some(Weight::Bold) } else { Some(Weight::Normal) },
                hover_weight: Some(Weight::Bold),
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
