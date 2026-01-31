//! Clickable wrapper component for mouse interaction
//!
//! Provides a generic wrapper that enables mouse event handling with automatic
//! hit-testing. Events are delivered only when they occur within component bounds,
//! and coordinates are relative to the component's top-left (0,0).

use iocraft::prelude::*;

/// Props for the Clickable component
#[derive(Default, Props)]
pub struct ClickableProps<'a> {
    /// Child element to wrap
    pub children: Vec<AnyElement<'a>>,
    /// Handler invoked when component is clicked
    pub on_click: Option<Handler<()>>,
    /// Handler invoked when mouse wheel scrolls up
    pub on_scroll_up: Option<Handler<()>>,
    /// Handler invoked when mouse wheel scrolls down
    pub on_scroll_down: Option<Handler<()>>,
}

/// Generic clickable wrapper component with automatic hit-testing
///
/// This component wraps children and provides mouse event handling:
/// - Click events (mouse down) trigger `on_click`
/// - Scroll up events trigger `on_scroll_up`
/// - Scroll down events trigger `on_scroll_down`
///
/// Uses `use_local_terminal_events` which provides automatic hit-testing:
/// - Events are only delivered when they occur within component bounds
/// - Mouse coordinates in events are relative to component's top-left (0,0)
///
/// # Example
///
/// ```ignore
/// use iocraft::prelude::*;
/// use janus::tui::components::Clickable;
///
/// // Create a handler using use_async_handler in your component
/// let click_handler = use_async_handler(move |()| {
///     // Handle click
/// });
///
/// element! {
///     Clickable(
///         on_click: Some(click_handler),
///     ) {
///         Text(content: "Click me")
///     }
/// }
/// ```
#[component]
pub fn Clickable<'a>(
    props: &mut ClickableProps<'a>,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'a>> {
    // Extract handlers from props, storing them in local variables
    let on_click = props.on_click.clone();
    let on_scroll_up = props.on_scroll_up.clone();
    let on_scroll_down = props.on_scroll_down.clone();

    hooks.use_local_terminal_events({
        move |event| {
            if let TerminalEvent::FullscreenMouse(mouse_event) = event {
                match mouse_event.kind {
                    MouseEventKind::Down(_) => {
                        if let Some(ref handler) = on_click {
                            handler(());
                        }
                    }
                    MouseEventKind::ScrollUp => {
                        if let Some(ref handler) = on_scroll_up {
                            handler(());
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        if let Some(ref handler) = on_scroll_down {
                            handler(());
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    // Return the first child if any, otherwise an empty view
    match props.children.iter_mut().next() {
        Some(child) => child.into(),
        None => element!(View).into_any(),
    }
}
