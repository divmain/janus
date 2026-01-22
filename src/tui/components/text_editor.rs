//! Multi-line text editor component
//!
//! Provides an editable multiline text input using iocraft's TextInput
//! with multiline support. Handles cursor positioning, scrolling, and
//! text manipulation automatically.

use iocraft::prelude::*;

use crate::tui::theme::theme;

/// Props for the TextEditor component
#[derive(Default, Props)]
pub struct TextEditorProps {
    /// Text content state (mutated by TextInput) - used for external sync
    pub value: Option<State<String>>,

    /// Whether the editor has focus
    pub has_focus: bool,

    /// Optional cursor color override (defaults to theme.highlight)
    pub cursor_color: Option<Color>,
}

/// Multi-line text editor with full cursor support
///
/// Uses an uncontrolled pattern internally to avoid cursor position issues
/// caused by the controlled input re-render cycle. The external state is
/// updated on every change for saving purposes, but the TextInput maintains
/// its own internal state and doesn't receive the value back as a prop.
#[component]
pub fn TextEditor(props: &TextEditorProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = theme();

    let Some(mut external_value) = props.value else {
        return element! {
            View(
                width: 100pct,
                flex_grow: 1.0,
                overflow: Overflow::Hidden,
            ) {
                Text(content: "No value state provided", color: theme.text_dimmed)
            }
        };
    };

    // Local state for TextInput - initialized from external value only once
    let initial_value = external_value.to_string();
    let mut local_value = hooks.use_state(move || initial_value);

    // Handle for imperative cursor control
    let mut handle = hooks.use_ref_default::<TextInputHandle>();

    // Set cursor to beginning on initial render
    hooks.use_effect(move || handle.write().set_cursor_offset(0), ());

    // Get current local value for TextInput
    let text_input_value = local_value.to_string();

    element! {
        View(
            width: 100pct,
            flex_grow: 1.0,
            overflow: Overflow::Hidden,
        ) {
            TextInput(
                has_focus: props.has_focus,
                value: text_input_value,
                on_change: move |new_value: String| {
                    // Update local state (this controls TextInput)
                    local_value.set(new_value.clone());
                    // Also update external state (for saving)
                    external_value.set(new_value);
                },
                multiline: true,
                cursor_color: props.cursor_color.or_else(|| Some(theme.highlight)),
                color: Some(theme.text),
                handle,
            )
        }
    }
}
