//! Empty state component
//!
//! Displays helpful messages when there are no tickets or when the
//! Janus directory doesn't exist.

use iocraft::prelude::*;

use crate::tui::repository::InitResult;
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

/// Compute which empty state to display based on current state
///
/// This function encapsulates the logic for determining whether to show an empty state
/// and which kind. It checks loading state, initialization results, and ticket counts
/// to decide between Loading, NoJanusDir, NoTickets, NoSearchResults, or no empty state.
///
/// # Arguments
///
/// * `is_loading` - Whether tickets are currently being loaded
/// * `init_result` - Result of repository initialization
/// * `all_ticket_count` - Total number of tickets in the repository
/// * `filtered_count` - Number of tickets after applying filters/search
/// * `query` - Current search query string
///
/// # Returns
///
/// * `Some(EmptyStateKind)` if an empty state should be shown
/// * `None` if there are tickets to display
pub fn compute_empty_state(
    is_loading: bool,
    init_result: InitResult,
    all_ticket_count: usize,
    filtered_count: usize,
    query: &str,
) -> Option<EmptyStateKind> {
    if is_loading {
        Some(EmptyStateKind::Loading)
    } else {
        match init_result {
            InitResult::NoJanusDir => Some(EmptyStateKind::NoJanusDir),
            InitResult::EmptyDir => {
                if all_ticket_count == 0 {
                    Some(EmptyStateKind::NoTickets)
                } else {
                    None
                }
            }
            InitResult::Ok => {
                if all_ticket_count == 0 {
                    Some(EmptyStateKind::NoTickets)
                } else if filtered_count == 0 && !query.is_empty() {
                    Some(EmptyStateKind::NoSearchResults)
                } else {
                    None
                }
            }
        }
    }
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

    #[test]
    fn test_compute_empty_state_loading() {
        let result = compute_empty_state(true, InitResult::Ok, 10, 10, "");
        assert_eq!(result, Some(EmptyStateKind::Loading));

        // Loading takes precedence over all other states
        let result = compute_empty_state(true, InitResult::NoJanusDir, 0, 0, "");
        assert_eq!(result, Some(EmptyStateKind::Loading));
    }

    #[test]
    fn test_compute_empty_state_no_janus_dir() {
        let result = compute_empty_state(false, InitResult::NoJanusDir, 0, 0, "");
        assert_eq!(result, Some(EmptyStateKind::NoJanusDir));
    }

    #[test]
    fn test_compute_empty_state_empty_dir() {
        // EmptyDir with no tickets
        let result = compute_empty_state(false, InitResult::EmptyDir, 0, 0, "");
        assert_eq!(result, Some(EmptyStateKind::NoTickets));

        // EmptyDir but tickets exist (shouldn't happen in practice)
        let result = compute_empty_state(false, InitResult::EmptyDir, 10, 10, "");
        assert_eq!(result, None);
    }

    #[test]
    fn test_compute_empty_state_no_tickets() {
        let result = compute_empty_state(false, InitResult::Ok, 0, 0, "");
        assert_eq!(result, Some(EmptyStateKind::NoTickets));
    }

    #[test]
    fn test_compute_empty_state_no_search_results() {
        let result = compute_empty_state(false, InitResult::Ok, 10, 0, "search query");
        assert_eq!(result, Some(EmptyStateKind::NoSearchResults));

        // Empty query with no filtered results should not show search results state
        let result = compute_empty_state(false, InitResult::Ok, 10, 0, "");
        assert_eq!(result, None);
    }

    #[test]
    fn test_compute_empty_state_has_tickets() {
        let result = compute_empty_state(false, InitResult::Ok, 10, 10, "");
        assert_eq!(result, None);

        let result = compute_empty_state(false, InitResult::Ok, 10, 5, "query");
        assert_eq!(result, None);
    }
}
