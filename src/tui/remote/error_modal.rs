//! Error detail modal for displaying full error messages
//!
//! Provides a modal dialog that displays detailed error information
//! including the error type and message.

use iocraft::prelude::*;

use crate::tui::components::{
    ModalBorderColor, ModalContainer, ModalHeight, ModalOverlay, ModalWidth,
};
use crate::tui::theme::theme;

/// Props for the ErrorDetailModal component
#[derive(Default, Props)]
pub struct ErrorDetailModalProps {
    pub error_type: String,
    pub error_message: String,
    /// Handler invoked when modal is closed via X button
    pub on_close: Option<Handler<()>>,
}

impl ErrorDetailModalProps {
    pub fn new(error_type: String, error_message: String, on_close: Option<Handler<()>>) -> Self {
        Self {
            error_type,
            error_message,
            on_close,
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
        ModalOverlay() {
            ModalContainer(
                width: Some(ModalWidth::Percent(70)),
                height: Some(ModalHeight::Percent(30)),
                border_color: Some(ModalBorderColor::Error),
                title: Some("Error Details".to_string()),
                title_color: Some(Color::Red),
                on_close: props.on_close.clone(),
            ) {
                // Error content
                View(flex_direction: FlexDirection::Column) {
                    Text(content: "Type:", color: Color::Yellow, weight: Weight::Bold)
                    Text(content: &props.error_type, color: theme.text)
                    Text(content: "")
                    Text(content: "Message:", color: Color::Yellow, weight: Weight::Bold)
                    Text(content: &props.error_message, color: Color::Red)
                }
            }
        }
    }
}
