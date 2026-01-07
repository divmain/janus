//! Help modal for displaying keyboard shortcuts
//!
//! Provides a modal dialog that displays all available keyboard shortcuts
//! for the remote TUI.

use iocraft::prelude::*;

use crate::tui::theme::theme;

/// Props for the HelpModal component
#[derive(Default, Props)]
pub struct HelpModalProps;

/// Help modal component
///
/// Displays a list of keyboard shortcuts organized by category.
#[component]
pub fn HelpModal<'a>(_props: &HelpModalProps, _hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = theme();

    let shortcuts = [
        (
            "Navigation",
            vec![
                ("j / ↓", "Move down"),
                ("k / ↑", "Move up"),
                ("g", "Jump to top"),
                ("G", "Jump to bottom"),
                ("Tab", "Switch Local ↔ Remote view"),
            ],
        ),
        (
            "Selection",
            vec![
                ("Space", "Toggle selection"),
                ("Enter", "Toggle detail pane"),
            ],
        ),
        (
            "Local Operations",
            vec![
                ("p", "Push ticket to remote"),
                ("u", "Unlink selected ticket(s)"),
            ],
        ),
        ("Remote Operations", vec![("a", "Adopt issue to local")]),
        (
            "Link & Sync",
            vec![
                ("l", "Link local ticket to remote issue"),
                ("s", "Sync selected linked items"),
            ],
        ),
        ("Search & Filter", vec![("/", "Focus search box")]),
        (
            "General",
            vec![
                ("P", "Switch provider (GitHub ↔ Linear)"),
                ("?", "Show this help"),
                ("q", "Quit"),
                ("Esc", "Close modal / cancel"),
            ],
        ),
    ];

    element! {
        View(
            width: 80pct,
            height: 40pct,
            background_color: theme.background,
            border_style: BorderStyle::Double,
            border_color: theme.border_focused,
            padding: 2,
            flex_direction: FlexDirection::Column,
        ) {
            // Header
            View(
                width: 100pct,
                padding_bottom: 1,
                border_edges: Edges::Bottom,
                border_style: BorderStyle::Single,
                border_color: theme.border,
            ) {
                Text(
                    content: "Keyboard Shortcuts",
                    color: Color::Cyan,
                    weight: Weight::Bold,
                )
                View(flex_grow: 1.0)
                Text(content: "Press Esc to close", color: theme.text_dimmed)
            }

            // Shortcuts list
            View(
                flex_grow: 1.0,
                width: 100pct,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
            ) {
                #(shortcuts.iter().flat_map(|(category, items)| {
                    let mut elements = vec![
                        element! {
                            View(
                                width: 100pct,
                                margin_top: 1,
                            ) {
                                Text(
                                    content: format!("{}:", category),
                                    color: Color::Yellow,
                                    weight: Weight::Bold,
                                )
                            }
                        }
                    ];

                    for (key, description) in items {
                        let key_str = key.to_string();
                        let desc_str = description.to_string();
                        elements.push(element! {
                            View(
                                width: 100pct,
                                flex_direction: FlexDirection::Row,
                                padding_left: 2,
                            ) {
                                Text(
                                    content: format!("{:<20}", key_str),
                                    color: theme.text,
                                    weight: Weight::Bold,
                                )
                                Text(
                                    content: desc_str,
                                    color: theme.text_dimmed,
                                )
                            }
                        });
                    }

                    elements
                }))
            }
        }
    }
}
