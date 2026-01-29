//! Help modal for displaying keyboard shortcuts
//!
//! Provides a modal dialog that displays all available keyboard shortcuts
//! for the remote TUI.

use iocraft::prelude::*;

use crate::tui::components::{
    ModalBorderColor, ModalContainer, ModalHeight, ModalOverlay, ModalWidth, TextViewer,
};

/// Props for the HelpModal component
#[derive(Default, Props)]
pub struct HelpModalProps {
    /// Current scroll offset (controlled by parent)
    pub scroll_offset: Option<usize>,
}

/// Help modal component
///
/// Displays a list of keyboard shortcuts organized by category.
/// Supports scrolling for long content via scroll_offset prop.
#[component]
pub fn HelpModal<'a>(props: &HelpModalProps, _hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let scroll_offset = props.scroll_offset.unwrap_or(0);

    // Build help text content
    let help_text = build_help_text();

    element! {
        ModalOverlay() {
            ModalContainer(
                width: Some(ModalWidth::Percent(60)),
                height: Some(ModalHeight::Percent(80)),
                border_color: Some(ModalBorderColor::Info),
                title: Some("Keyboard Shortcuts".to_string()),
                show_close_hint: Some(true),
                footer_text: Some("j/k or ↑/↓ to scroll".to_string()),
            ) {
                // Scrollable content using TextViewer
                View(
                    flex_grow: 1.0,
                    width: 100pct,
                    overflow: Overflow::Hidden,
                ) {
                    TextViewer(
                        text: help_text,
                        scroll_offset: scroll_offset,
                        has_focus: true,
                        placeholder: None,
                    )
                }
            }
        }
    }
}

/// Build the help text content as a single string
fn build_help_text() -> String {
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
                ("Ctrl+q", "Quit"),
                ("Esc", "Close modal / cancel"),
            ],
        ),
    ];

    let mut lines = Vec::new();

    for (category, items) in shortcuts {
        lines.push(String::new()); // Blank line before category
        lines.push(format!("{}:", category));

        for (key, description) in items {
            lines.push(format!("  {:<18} {}", key, description));
        }
    }

    // Remove leading blank line
    if !lines.is_empty() && lines[0].is_empty() {
        lines.remove(0);
    }

    lines.join("\n")
}

/// Get the total number of lines in the help content (for scroll bounds)
pub fn help_content_line_count() -> usize {
    build_help_text().lines().count()
}
