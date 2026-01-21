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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_press() {
        let event = key_press(KeyCode::Enter);
        match event {
            TerminalEvent::Key(key) => {
                assert_eq!(key.code, KeyCode::Enter);
                assert_eq!(key.kind, KeyEventKind::Press);
            }
            _ => panic!("Expected Key event"),
        }
    }

    #[test]
    fn test_key_release() {
        let event = key_release(KeyCode::Enter);
        match event {
            TerminalEvent::Key(key) => {
                assert_eq!(key.code, KeyCode::Enter);
                assert_eq!(key.kind, KeyEventKind::Release);
            }
            _ => panic!("Expected Key event"),
        }
    }

    #[test]
    fn test_resize() {
        let event = resize(80, 24);
        match event {
            TerminalEvent::Resize(w, h) => {
                assert_eq!(w, 80);
                assert_eq!(h, 24);
            }
            _ => panic!("Expected Resize event"),
        }
    }

    #[test]
    fn test_char_key() {
        let event = char_key('q');
        match event {
            TerminalEvent::Key(key) => {
                assert_eq!(key.code, KeyCode::Char('q'));
            }
            _ => panic!("Expected Key event"),
        }
    }

    #[test]
    fn test_escape() {
        let event = escape();
        match event {
            TerminalEvent::Key(key) => {
                assert_eq!(key.code, KeyCode::Esc);
            }
            _ => panic!("Expected Key event"),
        }
    }
}
