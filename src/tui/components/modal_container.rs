//! Modal container component
//!
//! Provides a standardized modal box structure with header, content area, and footer.

use iocraft::prelude::*;

use crate::tui::theme::theme;

/// Predefined modal border colors for common use cases
#[derive(Clone, Copy, Default)]
pub enum ModalBorderColor {
    #[default]
    Focused, // theme.border_focused (blue)
    Warning, // Yellow
    Error,   // Red
    Info,    // Cyan
}

impl ModalBorderColor {
    pub fn to_color(&self) -> Color {
        match self {
            Self::Focused => theme().border_focused,
            Self::Warning => Color::Yellow,
            Self::Error => Color::Red,
            Self::Info => Color::Cyan,
        }
    }
}

/// Modal width configuration
#[derive(Clone)]
pub enum ModalWidth {
    Fixed(u32),   // Fixed column count (e.g., 60)
    Percent(u32), // Percentage of terminal width (e.g., 70)
}

impl Default for ModalWidth {
    fn default() -> Self {
        Self::Fixed(60)
    }
}

/// Modal height configuration
#[derive(Clone, Default)]
pub enum ModalHeight {
    #[default]
    Auto, // Content-determined
    Fixed(u32),   // Fixed row count
    Percent(u32), // Percentage of terminal height
}

/// Props for the ModalContainer component
#[derive(Default, Props)]
pub struct ModalContainerProps<'a> {
    // Dimensions
    pub width: Option<ModalWidth>,
    pub height: Option<ModalHeight>,

    // Styling
    pub border_color: Option<ModalBorderColor>,

    // Header
    pub title: Option<String>,
    pub title_color: Option<Color>,
    pub show_close_hint: Option<bool>, // Shows "Press Esc to close"

    // Footer
    pub footer_text: Option<String>,

    // Children
    pub children: Vec<AnyElement<'a>>,
}

/// Modal container component
///
/// Provides a standardized modal box structure with:
/// - Optional header with title and close hint
/// - Flexible content area
/// - Optional footer
///
/// # Example
///
/// ```ignore
/// element! {
///     ModalOverlay {
///         ModalContainer(
///             title: "Confirmation".to_string(),
///             show_close_hint: true,
///             footer_text: "Enter to confirm, Esc to cancel".to_string(),
///         ) {
///             Text(content: "Are you sure?")
///         }
///     }
/// }
/// ```
#[component]
pub fn ModalContainer<'a>(props: &mut ModalContainerProps<'a>) -> impl Into<AnyElement<'a>> {
    let theme = theme();

    let border_color = props.border_color.unwrap_or_default().to_color();
    let title_color = props.title_color.unwrap_or(Color::Cyan);
    let show_close_hint = props.show_close_hint.unwrap_or(false);

    let width = props.width.clone().unwrap_or_default();
    let height = props.height.clone().unwrap_or_default();

    let has_title = props.title.is_some();
    let has_footer = props.footer_text.is_some();

    // Build the view with conditional width/height
    element! {
        View(
            width: match &width {
                ModalWidth::Fixed(n) => Size::Length(*n),
                ModalWidth::Percent(n) => Size::Percent(*n as f32),
            },
            height: match &height {
                ModalHeight::Auto => Size::Auto,
                ModalHeight::Fixed(n) => Size::Length(*n),
                ModalHeight::Percent(n) => Size::Percent(*n as f32),
            },
            background_color: theme.background,
            border_style: BorderStyle::Double,
            border_color: border_color,
            padding: 1,
            flex_direction: FlexDirection::Column,
        ) {
            // Header - if title provided
            #(if has_title {
                let title = props.title.clone().unwrap_or_default();
                Some(element! {
                    View(
                        width: 100pct,
                        padding_bottom: 1,
                        border_edges: Edges::Bottom,
                        border_style: BorderStyle::Single,
                        border_color: theme.border,
                        flex_direction: FlexDirection::Row,
                    ) {
                        Text(
                            content: title,
                            color: title_color,
                            weight: Weight::Bold,
                        )
                        View(flex_grow: 1.0)
                        #(if show_close_hint {
                            Some(element! {
                                Text(content: "Press Esc to close", color: theme.text_dimmed)
                            })
                        } else {
                            None
                        })
                    }
                })
            } else {
                None
            })

            // Content area
            View(
                flex_grow: 1.0,
                width: 100pct,
                flex_direction: FlexDirection::Column,
                overflow: Overflow::Hidden,
            ) {
                #(std::mem::take(&mut props.children))
            }

            // Footer - if footer_text provided
            #(if has_footer {
                let footer = props.footer_text.clone().unwrap_or_default();
                Some(element! {
                    View(
                        width: 100pct,
                        padding_top: 1,
                        border_edges: Edges::Top,
                        border_style: BorderStyle::Single,
                        border_color: theme.border,
                    ) {
                        Text(content: footer, color: theme.text_dimmed)
                    }
                })
            } else {
                None
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modal_border_color_default() {
        let color = ModalBorderColor::default();
        assert!(matches!(color, ModalBorderColor::Focused));
    }

    #[test]
    fn test_modal_border_color_to_color() {
        // Warning should be yellow
        assert!(matches!(
            ModalBorderColor::Warning.to_color(),
            Color::Yellow
        ));
        // Error should be red
        assert!(matches!(ModalBorderColor::Error.to_color(), Color::Red));
        // Info should be cyan
        assert!(matches!(ModalBorderColor::Info.to_color(), Color::Cyan));
    }

    #[test]
    fn test_modal_width_default() {
        let width = ModalWidth::default();
        assert!(matches!(width, ModalWidth::Fixed(60)));
    }

    #[test]
    fn test_modal_height_default() {
        let height = ModalHeight::default();
        assert!(matches!(height, ModalHeight::Auto));
    }

    #[test]
    fn test_modal_container_props_default() {
        let props = ModalContainerProps::default();
        assert!(props.title.is_none());
        assert!(props.footer_text.is_none());
        assert!(props.show_close_hint.is_none());
        assert!(props.border_color.is_none());
    }
}
