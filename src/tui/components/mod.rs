//! Shared TUI components
//!
//! This module contains reusable UI components for both the issue browser
//! and kanban board views.

pub mod empty_state;
pub mod footer;
pub mod header;
pub mod search_box;
pub mod select;
pub mod text_editor;
pub mod text_viewer;
pub mod ticket_card;
pub mod ticket_detail;
pub mod ticket_list;
pub mod toast;

pub use empty_state::{EmptyState, EmptyStateKind, EmptyStateProps, compute_empty_state};
pub use footer::{
    Footer, FooterProps, Shortcut, board_shortcuts, browser_shortcuts, edit_shortcuts,
    empty_shortcuts, search_shortcuts,
};
pub use header::{Header, HeaderProps};
pub use search_box::{InlineSearchBox, InlineSearchBoxProps, SearchBox, SearchBoxProps};
pub use select::{Select, SelectProps, Selectable, options_for};
pub use text_editor::{TextEditor, TextEditorProps};
pub use text_viewer::{TextViewer, TextViewerProps};
pub use ticket_card::{TicketCard, TicketCardProps};
pub use ticket_detail::{TicketDetail, TicketDetailProps};
pub use ticket_list::{TicketList, TicketListProps, TicketRow, TicketRowProps};
pub use toast::{Toast, ToastLevel, ToastNotification, ToastNotificationProps, render_toast};
