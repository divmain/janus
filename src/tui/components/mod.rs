//! Shared TUI components
//!
//! This module contains reusable UI components for both the issue browser
//! and kanban board views.

pub mod clickable;
pub mod clickable_text;
pub mod empty_state;
pub mod footer;
pub mod header;
pub mod modal_container;
pub mod modal_overlay;
pub mod modal_state;
pub mod search_box;
pub mod select;
pub mod shortcuts;
pub mod text_editor;
pub mod text_viewer;
pub mod ticket_card;
pub mod ticket_detail;
pub mod ticket_list;
pub mod toast;

pub use clickable::{Clickable, ClickableProps};
pub use clickable_text::{ClickableText, ClickableTextProps};
pub use empty_state::{EmptyState, EmptyStateKind, EmptyStateProps, compute_empty_state};
pub use footer::{
    Footer, FooterProps, Shortcut, board_shortcuts, browser_shortcuts,
    cancel_confirm_modal_shortcuts, confirm_dialog_shortcuts, edit_shortcuts, empty_shortcuts,
    error_modal_shortcuts, filter_modal_shortcuts, help_modal_shortcuts, link_mode_shortcuts,
    note_input_modal_shortcuts, search_shortcuts, sync_preview_shortcuts, triage_shortcuts,
};
pub use header::{Header, HeaderProps};
pub use modal_container::{
    ModalBorderColor, ModalContainer, ModalContainerProps, ModalHeight, ModalWidth,
};
pub use modal_overlay::{MODAL_BACKDROP, ModalOverlay, ModalOverlayProps};
pub use modal_state::{CacheErrorModalData, ModalState, NoteModalData, TicketModalData};
pub use search_box::{InlineSearchBox, InlineSearchBoxProps, SearchBox, SearchBoxProps};
pub use select::{Select, SelectProps, Selectable, options_for};
pub use shortcuts::ShortcutsBuilder;
pub use text_editor::{TextEditor, TextEditorProps};
pub use text_viewer::{TextViewer, TextViewerProps};
pub use ticket_card::{TicketCard, TicketCardProps};
pub use ticket_detail::{TicketDetail, TicketDetailProps};
pub use ticket_list::{TicketList, TicketListProps, TicketRow, TicketRowProps};
pub use toast::{Toast, ToastLevel, ToastNotification, ToastNotificationProps, render_toast};
