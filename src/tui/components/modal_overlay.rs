//! Modal overlay component
//!
//! Provides a base positioning component for modals with optional backdrop.

use iocraft::prelude::*;

/// Standard backdrop color for all modals
pub const MODAL_BACKDROP: Color = Color::Rgb {
    r: 30,
    g: 30,
    b: 30,
};

/// Props for the ModalOverlay component
#[derive(Default, Props)]
pub struct ModalOverlayProps<'a> {
    /// Whether to show the backdrop (default: false)
    /// Set to true if you want a solid backdrop that hides content behind the modal
    pub show_backdrop: Option<bool>,
    /// Children elements to render inside the overlay
    pub children: Vec<AnyElement<'a>>,
}

/// Modal overlay component that handles centering and optional backdrop
///
/// This is a base component for building modals. It provides:
/// - Full-screen absolute positioning
/// - Centered content
/// - Optional backdrop color
///
/// # Example
///
/// ```ignore
/// element! {
///     ModalOverlay(show_backdrop: true) {
///         View(width: 40, height: 10, border_style: BorderStyle::Double) {
///             Text(content: "Modal content")
///         }
///     }
/// }
/// ```
#[component]
pub fn ModalOverlay<'a>(props: &mut ModalOverlayProps<'a>) -> impl Into<AnyElement<'a>> {
    let show_backdrop = props.show_backdrop.unwrap_or(false);

    element! {
        View(
            width: 100pct,
            height: 100pct,
            position: Position::Absolute,
            top: 0,
            left: 0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            background_color: if show_backdrop { Some(MODAL_BACKDROP) } else { None },
        ) {
            #(std::mem::take(&mut props.children))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modal_backdrop_constant() {
        // Verify the backdrop color is the expected dark gray
        assert!(matches!(
            MODAL_BACKDROP,
            Color::Rgb {
                r: 30,
                g: 30,
                b: 30
            }
        ));
    }

    #[test]
    fn test_show_backdrop_default() {
        // Default should be false (transparent)
        let props = ModalOverlayProps::default();
        assert!(!props.show_backdrop.unwrap_or(false));
    }
}
