//! Error toast notification system

use iocraft::prelude::*;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub level: ToastLevel,
    pub timestamp: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Warning,
    Error,
}

impl Toast {
    pub fn new(message: String, level: ToastLevel) -> Self {
        Self {
            message,
            level,
            timestamp: Instant::now(),
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(message.into(), ToastLevel::Info)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(message.into(), ToastLevel::Warning)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(message.into(), ToastLevel::Error)
    }

    pub fn color(&self) -> Color {
        match self.level {
            ToastLevel::Info => Color::Cyan,
            ToastLevel::Warning => Color::Yellow,
            ToastLevel::Error => Color::Red,
        }
    }
}
