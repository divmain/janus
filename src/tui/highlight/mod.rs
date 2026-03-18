//! Markdown syntax highlighting for TUI text viewers.
//!
//! Transforms raw markdown strings into styled line segments suitable
//! for rendering with iocraft's `MixedText` component.

pub mod code;
pub mod markdown;
pub mod types;

pub use markdown::highlight_markdown;
pub use types::{StyledLine, StyledSegment};
