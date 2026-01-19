//! Toast notification system
//!
//! Provides reusable toast notification infrastructure for TUI views.

use iocraft::prelude::*;
use std::time::Instant;

/// A toast notification message
#[derive(Debug, Clone)]
pub struct Toast {
    /// The message to display
    pub message: String,
    /// The severity level of the toast
    pub level: ToastLevel,
    /// When the toast was created
    pub timestamp: Instant,
}

/// Severity level for toast notifications
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    /// Informational message
    Info,
    /// Warning message
    Warning,
    /// Error message
    Error,
    /// Success message
    Success,
}

impl Toast {
    /// Create a new toast with the given message and level
    pub fn new(message: String, level: ToastLevel) -> Self {
        Self {
            message,
            level,
            timestamp: Instant::now(),
        }
    }

    /// Create an info toast
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(message.into(), ToastLevel::Info)
    }

    /// Create a warning toast
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(message.into(), ToastLevel::Warning)
    }

    /// Create an error toast
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(message.into(), ToastLevel::Error)
    }

    /// Create a success toast
    pub fn success(message: impl Into<String>) -> Self {
        Self::new(message.into(), ToastLevel::Success)
    }

    /// Get the color associated with this toast's level
    pub fn color(&self) -> Color {
        match self.level {
            ToastLevel::Info => Color::Cyan,
            ToastLevel::Warning => Color::Yellow,
            ToastLevel::Error => Color::Red,
            ToastLevel::Success => Color::Green,
        }
    }
}

/// Props for the ToastNotification component
#[derive(Default, Props)]
pub struct ToastNotificationProps {
    /// The toast to display
    pub toast: Option<Toast>,
}

/// A reusable toast notification component
///
/// Renders a toast notification bar at the bottom of the view.
/// The toast is styled based on its level (info, warning, error, success).
#[component]
pub fn ToastNotification(props: &ToastNotificationProps) -> impl Into<AnyElement<'static>> {
    element! {
        View() {
            #(props.toast.as_ref().map(|t| {
                element! {
                    View(
                        width: 100pct,
                        height: 3,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        background_color: Color::Black,
                        border_edges: Edges::Top,
                        border_style: BorderStyle::Single,
                        border_color: t.color(),
                    ) {
                        Text(content: t.message.clone(), color: t.color())
                    }
                }
            }))
        }
    }
}

/// Render a toast notification as an optional element
///
/// This is a convenience function for use in element! macros where you need
/// to conditionally render a toast.
pub fn render_toast(toast: &Option<Toast>) -> Option<AnyElement<'static>> {
    toast.as_ref().map(|t| {
        element! {
            View(
                width: 100pct,
                height: 3,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                background_color: Color::Black,
                border_edges: Edges::Top,
                border_style: BorderStyle::Single,
                border_color: t.color(),
            ) {
                Text(content: t.message.clone(), color: t.color())
            }
        }
        .into_any()
    })
}
