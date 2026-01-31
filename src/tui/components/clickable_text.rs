//! Clickable text component with hover effects
//!
//! Wraps iocraft's `Text` component with click handling and visual hover feedback.
//! Changes text color and weight when the mouse hovers over the component.

use iocraft::prelude::*;

use crate::tui::theme::theme;

/// Props for the ClickableText component
#[derive(Default, Props)]
pub struct ClickableTextProps {
    /// The text content to display
    pub content: String,
    /// Handler invoked when text is clicked
    pub on_click: Option<Handler<()>>,
    /// Base color for the text (when not hovered)
    pub color: Option<Color>,
    /// Color when text is hovered
    pub hover_color: Option<Color>,
    /// Base font weight
    pub weight: Option<Weight>,
    /// Font weight when hovered
    pub hover_weight: Option<Weight>,
}

/// Text component with click handling and hover visual feedback
///
/// This component wraps a `Text` element with:
/// - Click detection via `use_local_terminal_events`
/// - Hover state tracking via `MouseEventKind::Moved` events
/// - Visual feedback (color/weight changes) on hover
///
/// The hover state is automatically reset when the mouse leaves the component
/// (component stops receiving `Moved` events from `use_local_terminal_events`).
///
/// # Example
///
/// ```ignore
/// use iocraft::prelude::*;
/// use janus::tui::components::ClickableText;
///
/// // Create a handler using use_async_handler in your component
/// let click_handler = use_async_handler(move |()| {
///     // Handle click
/// });
///
/// element! {
///     ClickableText(
///         content: "Click me!".to_string(),
///         on_click: Some(click_handler),
///         color: Some(Color::White),
///         hover_color: Some(Color::Blue),
///         weight: Some(Weight::Normal),
///         hover_weight: Some(Weight::Bold),
///     )
/// }
/// ```
#[component]
pub fn ClickableText(
    props: &ClickableTextProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = theme();

    // Track hover state
    let is_hovered = hooks.use_state(|| false);

    // Extract on_click handler
    let on_click = props.on_click.clone();

    // Set up local terminal events for click and hover detection
    hooks.use_local_terminal_events({
        let mut is_hovered = is_hovered;
        move |event| {
            if let TerminalEvent::FullscreenMouse(mouse_event) = event {
                match mouse_event.kind {
                    MouseEventKind::Moved => {
                        // Mouse moved within component bounds
                        if !is_hovered.get() {
                            is_hovered.set(true);
                        }
                    }
                    MouseEventKind::Down(_) => {
                        // Click detected
                        if let Some(ref handler) = on_click {
                            handler(());
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    // Determine current color and weight based on hover state
    let current_color = if is_hovered.get() {
        props.hover_color.unwrap_or(theme.border_focused)
    } else {
        props.color.unwrap_or(theme.text)
    };

    let current_weight = if is_hovered.get() {
        props.hover_weight.unwrap_or(Weight::Bold)
    } else {
        props.weight.unwrap_or(Weight::Normal)
    };

    element! {
        Text(
            content: props.content.clone(),
            color: current_color,
            weight: current_weight,
        )
    }
}
