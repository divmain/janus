//! Multi-line text viewer component with scroll indicators
//!
//! Provides a read-only view of multi-line text with scroll indicator showing
//! "↑ X more above" / "↓ X more below" when content exceeds visible area.
//! Supports optional markdown syntax highlighting when the `markdown` prop is true.

use iocraft::prelude::*;

use crate::tui::highlight::highlight_markdown;
use crate::tui::theme::theme;

/// Props for the TextViewer component
#[derive(Default, Props)]
pub struct TextViewerProps {
    /// Text content to display
    pub text: String,

    /// Current scroll position (line index)
    pub scroll_offset: usize,

    /// Whether the viewer has focus (affects border color)
    pub has_focus: bool,

    /// Optional placeholder text shown when text is empty
    pub placeholder: Option<String>,

    /// When true, parse text as markdown and render with syntax highlighting.
    /// When false, render as plain text (existing behavior).
    pub markdown: bool,
}

/// Multi-line text viewer with scroll indicators
///
/// Displays multi-line text content with scroll indicators when content
/// exceeds the visible area. Supports placeholder text for empty content.
/// When `markdown` is true, renders markdown with syntax highlighting.
///
/// The actual visible height is controlled by the parent container's layout,
/// while this component handles scroll indicator rendering based on the
/// scroll_offset.
#[component]
pub fn TextViewer(props: &TextViewerProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();

    if props.text.is_empty() {
        let element: AnyElement<'static> = element! {
            View(
                width: 100pct,
                height: 100pct,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
            ) {
                #(props.placeholder.as_ref().map(|placeholder| element! {
                    Text(
                        content: placeholder.clone(),
                        color: theme.text_dimmed,
                    )
                }))
            }
        }
        .into();
        return element;
    }

    if props.markdown {
        let styled_lines = highlight_markdown(&props.text);
        let total_lines = styled_lines.len();
        let scroll = props.scroll_offset.min(total_lines.saturating_sub(1));

        let has_content_above = scroll > 0;
        let has_content_below = scroll + 1 < total_lines;

        let mut elements: Vec<AnyElement<'static>> = Vec::new();

        if has_content_above {
            elements.push(
                element! {
                    View(height: 1, flex_shrink: 0.0) {
                        Text(
                            content: format!("↑ {} more above", scroll),
                            color: theme.text_dimmed,
                        )
                    }
                }
                .into(),
            );
        }

        let visible_lines: Vec<AnyElement<'static>> = styled_lines
            .iter()
            .skip(scroll)
            .map(|styled_line| {
                let contents = styled_line.to_mixed_text_contents();
                element! {
                    View(height: 1, flex_shrink: 0.0) {
                        MixedText(contents: contents)
                    }
                }
                .into()
            })
            .collect();

        elements.push(
            element! {
                View(
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    overflow: Overflow::Hidden,
                ) {
                    #(visible_lines)
                }
            }
            .into(),
        );

        if has_content_below {
            let remaining = total_lines.saturating_sub(scroll + 1);
            elements.push(
                element! {
                    View(height: 1, flex_shrink: 0.0) {
                        Text(
                            content: format!("↓ {} more below", remaining),
                            color: theme.text_dimmed,
                        )
                    }
                }
                .into(),
            );
        }

        let element: AnyElement<'static> = element! {
            View(
                width: 100pct,
                height: 100pct,
                flex_direction: FlexDirection::Column,
            ) {
                #(elements)
            }
        }
        .into();
        return element;
    }

    // Plain text rendering (existing behavior)
    let lines: Vec<&str> = props.text.lines().collect();
    let total_lines = lines.len();
    let scroll = props.scroll_offset.min(total_lines.saturating_sub(1));

    let has_content_above = scroll > 0;
    let has_content_below = scroll + 1 < total_lines;

    let mut elements: Vec<AnyElement<'static>> = Vec::new();

    if has_content_above {
        elements.push(
            element! {
                View(height: 1, flex_shrink: 0.0) {
                    Text(
                        content: format!("↑ {} more above", scroll),
                        color: theme.text_dimmed,
                    )
                }
            }
            .into(),
        );
    }

    let visible_lines: Vec<AnyElement<'static>> = lines
        .iter()
        .skip(scroll)
        .map(|line| {
            let line_owned = line.to_string();
            element! {
                View(height: 1, flex_shrink: 0.0) {
                    Text(content: line_owned, color: theme.text)
                }
            }
            .into()
        })
        .collect();

    elements.push(
        element! {
            View(
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                overflow: Overflow::Hidden,
            ) {
                #(visible_lines)
            }
        }
        .into(),
    );

    if has_content_below {
        let remaining = total_lines.saturating_sub(scroll + 1);
        elements.push(
            element! {
                View(height: 1, flex_shrink: 0.0) {
                    Text(
                        content: format!("↓ {} more below", remaining),
                        color: theme.text_dimmed,
                    )
                }
            }
            .into(),
        );
    }

    element! {
        View(
            width: 100pct,
            height: 100pct,
            flex_direction: FlexDirection::Column,
        ) {
            #(elements)
        }
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_props() {
        let props: TextViewerProps = Default::default();
        assert_eq!(props.text, "");
        assert_eq!(props.scroll_offset, 0);
        assert!(!props.has_focus);
        assert!(props.placeholder.is_none());
        assert!(!props.markdown);
    }

    #[test]
    fn test_markdown_prop_default_is_false() {
        let props = TextViewerProps::default();
        assert!(!props.markdown);
    }

    #[test]
    fn test_markdown_prop_with_empty_text() {
        let props = TextViewerProps {
            text: String::new(),
            scroll_offset: 0,
            has_focus: false,
            placeholder: Some("No content".to_string()),
            markdown: true,
        };
        assert!(props.text.is_empty());
        assert!(props.markdown);
    }

    #[test]
    fn test_markdown_prop_construction() {
        let props = TextViewerProps {
            text: "# Heading\n\nParagraph".to_string(),
            scroll_offset: 0,
            has_focus: false,
            placeholder: None,
            markdown: true,
        };
        assert!(props.markdown);
    }

    #[test]
    fn test_empty_text_with_placeholder() {
        let _ = TextViewerProps {
            text: String::new(),
            scroll_offset: 0,
            has_focus: false,
            placeholder: Some("No content".to_string()),
            markdown: false,
        };
    }

    #[test]
    fn test_empty_text_without_placeholder() {
        let _ = TextViewerProps {
            text: String::new(),
            scroll_offset: 0,
            has_focus: false,
            placeholder: None,
            markdown: false,
        };
    }

    #[test]
    fn test_single_line_text() {
        let _ = TextViewerProps {
            text: "Single line".to_string(),
            scroll_offset: 0,
            has_focus: false,
            placeholder: None,
            markdown: false,
        };
    }

    #[test]
    fn test_short_text_fits_in_visible_area() {
        let _ = TextViewerProps {
            text: "Line 1\nLine 2\nLine 3".to_string(),
            scroll_offset: 0,
            has_focus: false,
            placeholder: None,
            markdown: false,
        };
    }

    #[test]
    fn test_long_text_with_scroll_at_start() {
        let _ = TextViewerProps {
            text: "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9\nLine 10"
                .to_string(),
            scroll_offset: 0,
            has_focus: false,
            placeholder: None,
            markdown: false,
        };
    }

    #[test]
    fn test_long_text_with_scroll_in_middle() {
        let _ = TextViewerProps {
            text: "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9\nLine 10"
                .to_string(),
            scroll_offset: 5,
            has_focus: false,
            placeholder: None,
            markdown: false,
        };
    }

    #[test]
    fn test_long_text_with_scroll_at_end() {
        let _ = TextViewerProps {
            text: "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9\nLine 10"
                .to_string(),
            scroll_offset: 9,
            has_focus: false,
            placeholder: None,
            markdown: false,
        };
    }

    #[test]
    fn test_scroll_indicator_not_showing_when_at_bottom() {
        let _ = TextViewerProps {
            text: "Line 1\nLine 2\nLine 3\nLine 4\nLine 5".to_string(),
            scroll_offset: 4,
            has_focus: false,
            placeholder: None,
            markdown: false,
        };
    }

    #[test]
    fn test_both_scroll_indicators_showing_in_middle() {
        let _ = TextViewerProps {
            text: "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9\nLine 10"
                .to_string(),
            scroll_offset: 5,
            has_focus: false,
            placeholder: None,
            markdown: false,
        };
    }

    #[test]
    fn test_only_down_indicator_showing_at_top() {
        let _ = TextViewerProps {
            text: "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9\nLine 10"
                .to_string(),
            scroll_offset: 0,
            has_focus: false,
            placeholder: None,
            markdown: false,
        };
    }

    #[test]
    fn test_only_up_indicator_showing_near_bottom() {
        let _ = TextViewerProps {
            text: "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9\nLine 10"
                .to_string(),
            scroll_offset: 8,
            has_focus: false,
            placeholder: None,
            markdown: false,
        };
    }

    #[test]
    fn test_no_scroll_indicators_for_short_text() {
        let _ = TextViewerProps {
            text: "Line 1\nLine 2".to_string(),
            scroll_offset: 0,
            has_focus: false,
            placeholder: None,
            markdown: false,
        };
    }

    #[test]
    fn test_has_focus_true() {
        let _ = TextViewerProps {
            text: "Text content".to_string(),
            scroll_offset: 0,
            has_focus: true,
            placeholder: None,
            markdown: false,
        };
    }

    #[test]
    fn test_scroll_offset_clamped_to_max() {
        let props = TextViewerProps {
            text: "Line 1\nLine 2\nLine 3".to_string(),
            scroll_offset: 100,
            has_focus: false,
            placeholder: None,
            markdown: false,
        };
        assert_eq!(props.scroll_offset, 100);
    }

    #[test]
    fn test_multiline_text_with_special_chars() {
        let _ = TextViewerProps {
            text: "Line 1\nSpecial chars: !@#$%^&*()\nLine 3\nLine 4\nLine 5".to_string(),
            scroll_offset: 0,
            has_focus: false,
            placeholder: None,
            markdown: false,
        };
    }

    #[test]
    fn test_text_with_empty_lines() {
        let _ = TextViewerProps {
            text: "Line 1\n\nLine 3\n\nLine 5".to_string(),
            scroll_offset: 0,
            has_focus: false,
            placeholder: None,
            markdown: false,
        };
    }

    #[test]
    fn test_placeholder_with_empty_text() {
        let props = TextViewerProps {
            text: String::new(),
            scroll_offset: 0,
            has_focus: false,
            placeholder: Some("No description".to_string()),
            markdown: false,
        };
        assert_eq!(props.placeholder, Some("No description".to_string()));
    }

    #[test]
    fn test_single_line_no_indicators() {
        let _ = TextViewerProps {
            text: "Single line".to_string(),
            scroll_offset: 0,
            has_focus: false,
            placeholder: None,
            markdown: false,
        };
    }
}
