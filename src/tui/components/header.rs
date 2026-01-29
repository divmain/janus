//! App header bar component
//!
//! Displays the application title and optional ticket count.

use iocraft::prelude::*;

use crate::tui::theme::theme;

/// Props for the Header component
#[derive(Default, Props)]
pub struct HeaderProps<'a> {
    /// Title (defaults to "Janus")
    pub title: Option<&'a str>,

    /// Subtitle
    pub subtitle: Option<&'a str>,

    /// Ticket count
    pub ticket_count: Option<usize>,

    /// Extra elements to render on the right (before ticket count)
    pub extra: Option<Vec<AnyElement<'a>>>,

    /// Provider info (for remote screen) - owned string to avoid lifetime issues
    pub provider: Option<String>,

    /// Custom prefix before title
    pub prefix: Option<&'a str>,

    /// Whether triage mode is active
    pub triage_mode: bool,
}

/// App header bar showing title and ticket count
#[component]
pub fn Header<'a>(props: &mut HeaderProps<'a>) -> impl Into<AnyElement<'a>> {
    let theme = theme();

    // Build title
    let title = if let (Some(title), Some(provider)) = (props.title, props.provider.as_ref()) {
        // Remote screen: "janus remote [Provider]"
        format!("{} [{}]", title, provider)
    } else if let Some(title) = props.title {
        // Custom title
        title.to_string()
    } else if let Some(provider) = props.provider.as_ref() {
        // Just provider
        format!("Janus [{}]", provider)
    } else {
        // Default
        "Janus".to_string()
    };

    // Build prefix + title
    let title_display = if let Some(prefix) = props.prefix {
        format!("{} {}", prefix, title)
    } else {
        title
    };

    // Build the left side: title + optional subtitle
    let left_text = match props.subtitle {
        Some(sub) => format!("{} - {}", title_display, sub),
        None => title_display,
    };

    element! {
        View(
            width: 100pct,
            height: 1,
            flex_direction: FlexDirection::Row,
            flex_shrink: 0.0,
            justify_content: JustifyContent::SpaceBetween,
            padding_left: 1,
            padding_right: 1,
            background_color: if props.triage_mode { theme.status_next } else { theme.highlight },
        ) {
            View(flex_direction: FlexDirection::Row, gap: 1) {
                // Triage mode indicator
                #(if props.triage_mode {
                    Some(element! {
                        Text(
                            content: "[TRIAGE]",
                            color: theme.text,
                            weight: Weight::Bold,
                        )
                    })
                } else {
                    None
                })

                Text(
                    content: left_text,
                    color: theme.text,
                    weight: Weight::Bold,
                )
            }
            View(flex_direction: FlexDirection::Row, gap: 1) {
                // Extra elements (column toggles, help indicator, etc.)
                #(std::mem::take(&mut props.extra).unwrap_or_default())

                // Ticket count
                #(props.ticket_count.map(|count| element! {
                        Text(
                            content: format!("{} tickets", count),
                            color: theme.text_dimmed,
                        )
                    }))
            }
        }
    }
}
