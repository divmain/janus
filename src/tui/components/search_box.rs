//! Search input component with search icon
//!
//! A text input field with a search icon prefix for fuzzy searching tickets.

use iocraft::prelude::*;

use crate::tui::theme::theme;

/// Props for the SearchBox component
#[derive(Default, Props)]
pub struct SearchBoxProps {
    /// State for the search query value
    pub value: Option<State<String>>,
    /// Whether the search box has focus
    pub has_focus: bool,
}

/// Search input with magnifying glass icon
#[component]
pub fn SearchBox(props: &SearchBoxProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();
    let border_color = if props.has_focus {
        theme.border_focused
    } else {
        theme.border
    };

    let Some(mut value) = props.value else {
        return element! {
            View(
                flex_direction: FlexDirection::Row,
                border_style: BorderStyle::Round,
                border_color: border_color,
                padding_left: 1,
                padding_right: 1,
                height: 3,
            ) {
                Text(content: "No value state provided", color: theme.text_dimmed)
            }
        };
    };

    element! {
        View(
            flex_direction: FlexDirection::Row,
            border_style: BorderStyle::Round,
            border_color: border_color,
            padding_left: 1,
            padding_right: 1,
            height: 3,
        ) {
            // Search icon (magnifying glass represented as "/")
            View(
                margin_right: 1,
                justify_content: JustifyContent::Center,
            ) {
                Text(
                    content: "/",
                    color: theme.text_dimmed,
                )
            }
            // Text input
            View(flex_grow: 1.0) {
                TextInput(
                    value: value.to_string(),
                    has_focus: props.has_focus,
                    on_change: move |new_value| value.set(new_value),
                    color: theme.text,
                )
            }
        }
    }
}

/// A simpler inline search box without borders (for kanban board header)
#[derive(Default, Props)]
pub struct InlineSearchBoxProps {
    /// State for the search query value
    pub value: Option<State<String>>,
    /// Whether the search box has focus
    pub has_focus: bool,
}

/// Inline search input without borders
#[component]
pub fn InlineSearchBox(props: &InlineSearchBoxProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();
    let has_focus = props.has_focus;

    let Some(mut value) = props.value else {
        return element! {
            View(flex_direction: FlexDirection::Row, height: 1) {
                Text(content: "No value state provided", color: theme.text_dimmed)
            }
        };
    };

    element! {
        View(
            flex_direction: FlexDirection::Row,
            width: 100pct,
            height: 1,
        ) {
            View(
                margin_right: 1,
                justify_content: JustifyContent::Center,
            ) {
                Text(
                    content: "/",
                    color: if has_focus { theme.border_focused } else { theme.text_dimmed },
                )
            }

            View(flex_grow: 1.0) {
                TextInput(
                    value: value.to_string(),
                    has_focus: has_focus,
                    on_change: move |new_value| value.set(new_value),
                    color: theme.text,
                )
            }
        }
    }
}
