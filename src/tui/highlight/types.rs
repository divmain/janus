//! Core types for styled text rendering.

use iocraft::prelude::*;

/// A single styled span of text within a line.
#[derive(Debug, Clone, PartialEq)]
pub struct StyledSegment {
    pub text: String,
    pub color: Option<Color>,
    pub weight: Weight,
    pub italic: bool,
    pub decoration: TextDecoration,
}

impl StyledSegment {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: None,
            weight: Weight::Normal,
            italic: false,
            decoration: TextDecoration::None,
        }
    }

    pub fn colored(text: impl Into<String>, color: Color) -> Self {
        Self {
            text: text.into(),
            color: Some(color),
            weight: Weight::Normal,
            italic: false,
            decoration: TextDecoration::None,
        }
    }

    pub fn bold(mut self) -> Self {
        self.weight = Weight::Bold;
        self
    }

    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub fn underline(mut self) -> Self {
        self.decoration = TextDecoration::Underline;
        self
    }

    pub fn to_mixed_text_content(&self) -> MixedTextContent {
        let mut content = MixedTextContent::new(&self.text);
        if let Some(color) = self.color {
            content = content.color(color);
        }
        content = content.weight(self.weight);
        if self.italic {
            content = content.italic();
        }
        content = content.decoration(self.decoration);
        content
    }
}

/// A complete styled line, ready for rendering as a MixedText element.
#[derive(Debug, Clone, PartialEq)]
pub struct StyledLine {
    pub segments: Vec<StyledSegment>,
}

impl StyledLine {
    pub fn new(segments: Vec<StyledSegment>) -> Self {
        Self { segments }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            segments: vec![StyledSegment::plain(text)],
        }
    }

    pub fn empty() -> Self {
        Self {
            segments: vec![StyledSegment::plain("")],
        }
    }

    pub fn to_mixed_text_contents(&self) -> Vec<MixedTextContent> {
        self.segments
            .iter()
            .map(|s| s.to_mixed_text_content())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_segment() {
        let seg = StyledSegment::plain("hello");
        assert_eq!(seg.text, "hello");
        assert_eq!(seg.color, None);
        assert_eq!(seg.weight, Weight::Normal);
        assert!(!seg.italic);
        assert_eq!(seg.decoration, TextDecoration::None);
    }

    #[test]
    fn test_colored_segment() {
        let seg = StyledSegment::colored("code", Color::DarkYellow);
        assert_eq!(seg.text, "code");
        assert_eq!(seg.color, Some(Color::DarkYellow));
    }

    #[test]
    fn test_bold_italic_chain() {
        let seg = StyledSegment::colored("x", Color::Red).bold().italic();
        assert_eq!(seg.weight, Weight::Bold);
        assert!(seg.italic);
        assert_eq!(seg.color, Some(Color::Red));
    }

    #[test]
    fn test_to_mixed_text_content() {
        let seg = StyledSegment::colored("test", Color::Blue).bold();
        let content = seg.to_mixed_text_content();
        assert!(!content.text.is_empty());
    }

    #[test]
    fn test_styled_line_plain() {
        let line = StyledLine::plain("single");
        assert_eq!(line.segments.len(), 1);
        assert_eq!(line.segments[0].text, "single");
    }

    #[test]
    fn test_styled_line_multi_segment() {
        let line = StyledLine::new(vec![
            StyledSegment::plain("a"),
            StyledSegment::colored("b", Color::Red),
        ]);
        let contents = line.to_mixed_text_contents();
        assert_eq!(contents.len(), 2);
    }

    #[test]
    fn test_styled_line_empty() {
        let line = StyledLine::empty();
        assert_eq!(line.segments.len(), 1);
        assert_eq!(line.segments[0].text, "");
    }
}
