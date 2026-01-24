//! TUI module for interactive terminal interfaces
//!
//! This module provides three main views:
//! - `view` - Issue browser with fuzzy search and inline editing
//! - `board` - Kanban board with column-based ticket organization
//! - `remote` - Remote TUI for managing local tickets and remote issues

pub mod action_queue;
pub mod analytics;
pub mod board;
pub mod components;
pub mod edit;
pub mod edit_state;
pub mod handlers;
pub mod hooks;
pub mod navigation;
pub mod remote;
pub mod repository;
pub mod search;
pub mod services;
pub mod state;
pub mod theme;
pub mod view;

pub use analytics::{StatusCounts, TicketAnalytics};
pub use board::{KanbanBoard, KanbanBoardProps};
pub use edit::{
    EditField, EditForm, EditFormOverlay, EditFormProps, EditResult, extract_body_for_edit,
};
pub use remote::RemoteTui;
pub use repository::{InitResult, TicketRepository};
pub use search::{FilteredItem, FilteredTicket, filter_items, filter_tickets};
pub use services::TicketService;
pub use state::{Pane, TuiState};
pub use theme::Theme;
pub use view::{IssueBrowser, IssueBrowserProps};
