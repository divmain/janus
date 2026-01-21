//! App header bar component
//!
//! Displays the application title and optional ticket count.

use iocraft::prelude::*;

use crate::tui::theme::theme;

/// Props for the Header component
#[derive(Default, Props)]
pub struct HeaderProps<'a> {
    /// Title to display (defaults to "Janus")
    pub title: Option<&'a str>,
    /// Optional subtitle (e.g., "Board" or "Browser")
    pub subtitle: Option<&'a str>,
    /// Optional ticket count to display on the right
    pub ticket_count: Option<usize>,
}

/// App header bar showing title and ticket count
#[component]
pub fn Header<'a>(props: &HeaderProps<'a>) -> impl Into<AnyElement<'a>> {
    let theme = theme();
    let title = props.title.unwrap_or("Janus");
    let subtitle = props.subtitle;

    // Build the left side: title + optional subtitle
    let left_text = match subtitle {
        Some(sub) => format!("{} - {}", title, sub),
        None => title.to_string(),
    };

    // Build the right side: ticket count
    let right_text = props
        .ticket_count
        .map(|count| format!("{} tickets", count))
        .unwrap_or_default();

    element! {
        View(
            width: 100pct,
            height: 1,
            flex_direction: FlexDirection::Row,
            flex_shrink: 0.0,
            justify_content: JustifyContent::SpaceBetween,
            padding_left: 1,
            padding_right: 1,
            background_color: theme.highlight,
        ) {
            View {
                Text(
                    content: left_text,
                    color: theme.text,
                    weight: Weight::Bold,
                )
            }
            View {
                Text(
                    content: right_text,
                    color: theme.text_dimmed,
                )
            }
        }
    }
}
