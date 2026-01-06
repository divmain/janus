//! Error detail modal for displaying full error messages
//!
//! Provides a modal dialog that displays detailed error information
//! including the error type and message.

use iocraft::prelude::*;

use crate::tui::theme::theme;

/// Props for the ErrorDetailModal component
#[derive(Default, Props)]
pub struct ErrorDetailModalProps {
    pub error_type: String,
    pub error_message: String,
}

impl ErrorDetailModalProps {
    pub fn new(error_type: String, error_message: String) -> Self {
        Self {
            error_type,
            error_message,
        }
    }
}

/// Error detail modal component
///
/// Displays a detailed error message in a modal dialog.
#[component]
pub fn ErrorDetailModal<'a>(
    props: &ErrorDetailModalProps,
    _hooks: Hooks,
) -> impl Into<AnyElement<'a>> {
    let theme = theme();

    element! {
        View(
            width: 70pct,
            height: 30pct,
            background_color: theme.background,
            border_style: BorderStyle::Double,
            border_color: Color::Red,
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
                    content: "Error Details",
                    color: Color::Red,
                    weight: Weight::Bold,
                )
                View(flex_grow: 1.0)
                Text(content: "Press Esc to close", color: theme.text_dimmed)
            }

            // Error content
            View(flex_grow: 1.0, width: 100pct, flex_direction: FlexDirection::Column) {
                Text(content: "Type:", color: Color::Yellow, weight: Weight::Bold)
                Text(content: &props.error_type, color: theme.text)
                Text(content: "")
                Text(content: "Message:", color: Color::Yellow, weight: Weight::Bold)
                Text(content: &props.error_message, color: Color::Red)
            }
        }
    }
}
