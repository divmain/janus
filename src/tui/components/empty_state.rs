//! Empty state component
//!
//! Displays helpful messages when there are no tickets or when the
//! Janus directory doesn't exist.

use iocraft::prelude::*;

use crate::tui::theme::theme;

/// Type of empty state to display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmptyStateKind {
    /// No .janus directory found
    NoJanusDir,
    /// .janus directory exists but no tickets
    #[default]
    NoTickets,
    /// No tickets match the search filter
    NoSearchResults,
    /// Loading tickets
    Loading,
}

/// Props for the EmptyState component
#[derive(Default, Props)]
pub struct EmptyStateProps {
    /// The kind of empty state to display
    pub kind: EmptyStateKind,
    /// Optional search query (for NoSearchResults)
    pub search_query: Option<String>,
}

/// Empty state display with helpful message
#[component]
pub fn EmptyState(props: &EmptyStateProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();

    let (icon, title, message, hint) = match props.kind {
        EmptyStateKind::NoJanusDir => (
            "!",
            "No Janus Directory",
            "The .janus directory was not found in the current path.",
            "Run 'janus create <title>' to create your first ticket.",
        ),
        EmptyStateKind::NoTickets => (
            "i",
            "No Tickets",
            "Your ticket tracker is empty.",
            "Press 'n' to create a new ticket, or run 'janus create <title>'.",
        ),
        EmptyStateKind::NoSearchResults => (
            "?",
            "No Results",
            "No tickets match your search.",
            "Try a different search term, or press Esc to clear.",
        ),
        EmptyStateKind::Loading => ("~", "Loading", "Loading tickets...", ""),
    };

    element! {
        View(
            width: 100pct,
            height: 100pct,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            padding: 2,
        ) {
            // Icon in a box
            View(
                width: 5,
                height: 3,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_style: BorderStyle::Round,
                border_color: if props.kind == EmptyStateKind::NoJanusDir {
                    theme.priority_p0
                } else {
                    theme.border
                },
                margin_bottom: 1,
            ) {
                Text(
                    content: icon,
                    color: if props.kind == EmptyStateKind::NoJanusDir {
                        theme.priority_p0
                    } else {
                        theme.text_dimmed
                    },
                    weight: Weight::Bold,
                )
            }

            // Title
            Text(
                content: title,
                color: theme.text,
                weight: Weight::Bold,
            )

            // Message
            View(margin_top: 1, max_width: 60) {
                Text(
                    content: message,
                    color: theme.text_dimmed,
                )
            }

            // Search query (if applicable)
            #(if props.kind == EmptyStateKind::NoSearchResults && props.search_query.is_some() {
                let query = props.search_query.clone().unwrap_or_default();
                Some(element! {
                    View(margin_top: 1) {
                        Text(
                            content: format!("Search: \"{}\"", query),
                            color: theme.search_match,
                        )
                    }
                })
            } else {
                None
            })

            // Hint
            #(if !hint.is_empty() {
                Some(element! {
                    View(margin_top: 2) {
                        Text(
                            content: hint,
                            color: theme.text_dimmed,
                        )
                    }
                })
            } else {
                None
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_state_kind_default() {
        let kind = EmptyStateKind::default();
        assert_eq!(kind, EmptyStateKind::NoTickets);
    }
}
