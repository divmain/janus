//! Confirmation dialog for remote TUI operations

use iocraft::prelude::*;

use crate::tui::components::TextViewer;

#[derive(Debug, Clone, Props)]
pub struct ConfirmDialogState {
    pub message: String,
    pub default_yes: bool,
}

impl ConfirmDialogState {
    pub fn new(message: String, default_yes: bool) -> Self {
        Self {
            message,
            default_yes,
        }
    }
}

/// Confirmation dialog component
#[component]
pub fn ConfirmDialog<'a>(props: &ConfirmDialogState, _hooks: Hooks) -> impl Into<AnyElement<'a>> {
    element! {
        View(
            width: 100pct,
            height: 100pct,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            background_color: Color::Black,
        ) {
            View(
                width: 60,
                border_style: BorderStyle::Double,
                border_color: Color::Yellow,
                padding: 1,
                flex_direction: FlexDirection::Column,
                background_color: Color::Rgb { r: 120, g: 120, b: 120 },
            ) {
                Text(
                    content: "Confirm",
                    color: Color::Yellow,
                    weight: Weight::Bold,
                )
                Text(content: "")

                TextViewer(
                    text: props.message.clone(),
                    scroll_offset: 0usize,
                    has_focus: false,
                    placeholder: Some("No message".to_string()),
                )

                Text(content: "")
                Text(
                    content: "[Y]es / [n]o / [c]ancel",
                    color: Color::Cyan,
                )
            }
        }
    }
}
