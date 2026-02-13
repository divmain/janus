//! Synchronous handler utilities for state updates
//!
//! This module provides utilities for creating synchronous handlers.
//! However, due to iocraft's architecture where Handler<T> requires Fn (immutable)
//! but State::set() requires &mut self, state mutations must use use_async_handler.
//!
//! The Cycleable trait and implementations here support the edit form's cycling behavior.

use crate::types::{TicketPriority, TicketStatus, TicketType};

/// Trait for types that can be cycled (next/prev)
pub trait Cycleable: Copy + Send + Sync + 'static {
    fn next(self) -> Self;
    fn prev(self) -> Self;
}

impl Cycleable for TicketStatus {
    fn next(self) -> Self {
        self.next()
    }

    fn prev(self) -> Self {
        self.prev()
    }
}

impl Cycleable for TicketType {
    fn next(self) -> Self {
        self.next()
    }

    fn prev(self) -> Self {
        self.prev()
    }
}

impl Cycleable for TicketPriority {
    fn next(self) -> Self {
        self.next()
    }

    fn prev(self) -> Self {
        self.prev()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ticket_status_cycle() {
        assert_eq!(TicketStatus::New.next(), TicketStatus::Next);
        assert_eq!(TicketStatus::Next.prev(), TicketStatus::New);
        assert_eq!(TicketStatus::Cancelled.next(), TicketStatus::New);
        assert_eq!(TicketStatus::New.prev(), TicketStatus::Cancelled);
    }

    #[test]
    fn test_ticket_type_cycle() {
        assert_eq!(TicketType::Bug.next(), TicketType::Feature);
        assert_eq!(TicketType::Task.prev(), TicketType::Feature);
    }

    #[test]
    fn test_ticket_priority_cycle() {
        assert_eq!(TicketPriority::P0.next(), TicketPriority::P1);
        assert_eq!(TicketPriority::P4.next(), TicketPriority::P0);
        assert_eq!(TicketPriority::P0.prev(), TicketPriority::P4);
    }
}
