//! Base abstraction for TUI screens
//!
//! This module provides common functionality and types shared across all TUI screens
//! (IssueBrowser, KanbanBoard, RemoteTui). Instead of a complex macro, we use:
//!
//! 1. `ScreenState` - Common state that all screens need
//! 2. `ScreenLayout` - Helper component for consistent screen structure
//! 3. Helper functions for common operations
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::tui::screen_base::{ScreenState, ScreenLayout, use_screen_state};
//!
//! #[component]
//! fn MyScreen<'a>(_props: &MyScreenProps, mut hooks: Hooks) -> impl Into<AnyElement<'a>> {
//!     // Initialize common state
//!     let screen = use_screen_state(&mut hooks);
//!
//!     // Use the screen layout wrapper
//!     element! {
//!         ScreenLayout(
//!             width: screen.width,
//!             height: screen.height,
//!             header_subtitle: Some("My Screen"),
//!             shortcuts: my_shortcuts(),
//!             toast: toast.read().clone(),
//!         ) {
//!             // Your screen-specific content here
//!         }
//!     }
//! }
//! ```

use iocraft::prelude::*;

use crate::tui::components::{Footer, Header, Shortcut, ToastNotification};
use crate::tui::theme::theme;

/// Common state that all TUI screens need
///
/// This struct bundles the frequently-used state values that every screen requires,
/// reducing boilerplate in individual screen implementations.
pub struct ScreenState {
    /// Terminal width
    pub width: u16,
    /// Terminal height
    pub height: u16,
    /// Whether the screen should exit
    pub should_exit: State<bool>,
    /// Whether data needs to be reloaded
    pub needs_reload: State<bool>,
    /// Whether the screen is currently loading
    pub is_loading: State<bool>,
}

impl ScreenState {
    /// Check if should_exit is set and return true if the screen should exit
    pub fn check_exit(&self) -> bool {
        self.should_exit.get()
    }

    /// Trigger a reload of the screen's data
    pub fn trigger_reload(&mut self) {
        self.needs_reload.set(true);
    }

    /// Check if a reload is pending and clear the flag
    ///
    /// Returns true if a reload was pending, false otherwise.
    /// Use this in screens to check if they need to refresh data.
    pub fn consume_reload(&mut self) -> bool {
        if self.needs_reload.get() && !self.is_loading.get() {
            self.needs_reload.set(false);
            self.is_loading.set(true);
            true
        } else {
            false
        }
    }
}

/// Initialize common screen state
///
/// This function sets up all the common state that screens need, returning
/// a `ScreenState` struct. Individual screens can then add their own
/// screen-specific state on top of this.
///
/// # Example
///
/// ```rust,ignore
/// let screen = use_screen_state(&mut hooks);
/// let my_custom_state = hooks.use_state(|| MyCustomState::default());
/// ```
pub fn use_screen_state(hooks: &mut Hooks) -> ScreenState {
    let (width, height) = hooks.use_terminal_size();
    let should_exit = hooks.use_state(|| false);
    let needs_reload = hooks.use_state(|| false);
    let is_loading = hooks.use_state(|| true);

    ScreenState {
        width,
        height,
        should_exit,
        needs_reload,
        is_loading,
    }
}

/// Handle the exit check for a screen
///
/// This should be called after event processing. If `should_exit` is true,
/// it will call `system.exit()` to terminate the application.
///
/// # Example
///
/// ```rust,ignore
/// let mut system = hooks.use_context_mut::<SystemContext>();
/// // After event handling
/// handle_screen_exit(&screen, &mut system);
/// ```
pub fn handle_screen_exit<S: AsMut<SystemContext>>(screen: &ScreenState, system: &mut S) {
    if screen.check_exit() {
        system.as_mut().exit();
    }
}

/// Props for the ScreenLayout component
#[derive(Default, Props)]
pub struct ScreenLayoutProps<'a> {
    /// Terminal width
    pub width: u16,
    /// Terminal height
    pub height: u16,

    /// Header title (defaults to "Janus")
    pub header_title: Option<&'a str>,
    /// Header subtitle (e.g., "Browser", "Board")
    pub header_subtitle: Option<&'a str>,
    /// Ticket count for header
    pub header_ticket_count: Option<usize>,
    /// Extra header elements
    pub header_extra: Option<Vec<AnyElement<'a>>>,
    /// Provider info for header (remote screen)
    pub header_provider: Option<String>,

    /// Keyboard shortcuts for footer
    pub shortcuts: Vec<Shortcut>,

    /// Toast notification to display
    pub toast: Option<crate::tui::components::Toast>,

    /// Whether triage mode is active
    pub triage_mode: bool,

    /// The main content of the screen
    pub children: Vec<AnyElement<'a>>,
}

/// Standard screen layout component
///
/// Provides the common structure for all TUI screens:
/// - Header at top
/// - Content area (children)
/// - Toast notification overlay
/// - Footer at bottom
///
/// This component handles the boilerplate layout that's identical across screens,
/// allowing each screen to focus on its unique content.
#[component]
pub fn ScreenLayout<'a>(props: &mut ScreenLayoutProps<'a>) -> impl Into<AnyElement<'a>> {
    let theme = theme();

    // Take ownership of children to avoid borrow issues
    let children = std::mem::take(&mut props.children);
    let header_extra = std::mem::take(&mut props.header_extra);
    let toast = props.toast.clone();
    let shortcuts = std::mem::take(&mut props.shortcuts);

    element! {
        View(
            width: props.width,
            height: props.height,
            flex_direction: FlexDirection::Column,
            background_color: theme.background,
            position: Position::Relative,
        ) {
            // Header
            Header(
                title: props.header_title,
                subtitle: props.header_subtitle,
                ticket_count: props.header_ticket_count,
                extra: header_extra,
                provider: props.header_provider.clone(),
                triage_mode: props.triage_mode,
            )

            // Main content area
            View(
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                width: 100pct,
                overflow: Overflow::Hidden,
            ) {
                #(children)
            }

            // Toast notification (rendered before footer so it appears above content)
            #(if toast.is_some() {
                Some(element! {
                    ToastNotification(toast: toast)
                })
            } else {
                None
            })

            // Footer
            Footer(shortcuts: shortcuts)
        }
    }
}

/// Helper to check if a key event should be processed
///
/// Returns false if the event is a key release (which should be ignored).
/// Screens can use this as the first check in their event handlers.
pub fn should_process_key_event(kind: KeyEventKind) -> bool {
    kind != KeyEventKind::Release
}

/// Calculate the available list height for a screen
///
/// Subtracts standard UI elements from total height:
/// - Header: 1 line
/// - Footer: 1 line
/// - Additional elements as specified
///
/// # Arguments
///
/// * `total_height` - The total terminal height
/// * `additional_elements` - Number of additional lines used by screen-specific elements
///   (search box, tabs, selection bars, etc.)
pub fn calculate_list_height(total_height: u16, additional_elements: u16) -> usize {
    // Header (1) + Footer (1) + additional elements
    total_height.saturating_sub(2 + additional_elements) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_list_height() {
        // Standard case: 24 line terminal, search box (3 lines)
        assert_eq!(calculate_list_height(24, 3), 19);

        // Edge case: very small terminal
        assert_eq!(calculate_list_height(5, 3), 0);

        // Board view with more elements
        assert_eq!(calculate_list_height(40, 5), 33);
    }

    #[test]
    fn test_should_process_key_event() {
        assert!(should_process_key_event(KeyEventKind::Press));
        assert!(should_process_key_event(KeyEventKind::Repeat));
        assert!(!should_process_key_event(KeyEventKind::Release));
    }
}
