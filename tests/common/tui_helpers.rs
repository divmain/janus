//! TUI testing helpers for iocraft-based components.
//!
//! This module provides utilities for testing TUI components including
//! event simulation helpers.

#![allow(dead_code)]

use iocraft::prelude::*;

/// Build a key press event
pub fn key_press(code: KeyCode) -> TerminalEvent {
    TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, code))
}

/// Build a key release event
pub fn key_release(code: KeyCode) -> TerminalEvent {
    TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, code))
}

/// Build a resize event
pub fn resize(width: u16, height: u16) -> TerminalEvent {
    TerminalEvent::Resize(width, height)
}

/// Build an Escape key press event
pub fn escape() -> TerminalEvent {
    key_press(KeyCode::Esc)
}

/// Build an Enter key press event
pub fn enter() -> TerminalEvent {
    key_press(KeyCode::Enter)
}

/// Build a Tab key press event
pub fn tab() -> TerminalEvent {
    key_press(KeyCode::Tab)
}

/// Build an arrow up key press event
pub fn arrow_up() -> TerminalEvent {
    key_press(KeyCode::Up)
}

/// Build an arrow down key press event
pub fn arrow_down() -> TerminalEvent {
    key_press(KeyCode::Down)
}

/// Build an arrow left key press event
pub fn arrow_left() -> TerminalEvent {
    key_press(KeyCode::Left)
}

/// Build an arrow right key press event
pub fn arrow_right() -> TerminalEvent {
    key_press(KeyCode::Right)
}

/// Build a character key press event
pub fn char_key(c: char) -> TerminalEvent {
    key_press(KeyCode::Char(c))
}

/// Build a backspace key press event
pub fn backspace() -> TerminalEvent {
    key_press(KeyCode::Backspace)
}

// Note: Self-tests for this module have been intentionally removed.
// This module is included via #[path] into every test binary, so any tests
// here would be duplicated 10+ times across all test binaries.
