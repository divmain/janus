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
/// Wraps iocraft's TextInput with multiline mode enabled. The external state
/// is passed directly to TextInput and updated on every change. TextInput's
/// internal `new_cursor_offset` logic handles cursor positioning when the
/// value changes. On initial render, the cursor is set to position 0.
#[component]
pub fn TextEditor(props: &TextEditorProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = theme();

    let Some(mut text_input_value) = props.value else {
        // let Some(mut external_value) = props.value else {
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

    // Handle for imperative cursor control
    let mut handle = hooks.use_ref_default::<TextInputHandle>();

    // Set cursor to beginning on initial render only
    // Note: () as dependency means "run once after first render" per iocraft docs
    hooks.use_effect(move || handle.write().set_cursor_offset(0), ());

    element! {
        View(
            width: 100pct,
            flex_grow: 1.0,
            overflow: Overflow::Hidden,
        ) {
            TextInput(
                has_focus: props.has_focus,
                value: text_input_value.to_string(),
                on_change: move |new_value: String| {
                    text_input_value.set(new_value);
                },
                multiline: true,
                cursor_color: props.cursor_color.or_else(|| Some(theme.highlight)),
                color: Some(theme.text),
                handle,
            )
        }
    }
}
