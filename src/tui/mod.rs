//! TUI module for interactive terminal interfaces
//!
//! This module provides two main views:
//! - `view` - Issue browser with fuzzy search and inline editing
//! - `board` - Kanban board with column-based ticket organization

pub mod board;
pub mod components;
pub mod edit;
pub mod search;
pub mod state;
pub mod theme;
pub mod view;

pub use board::{KanbanBoard, KanbanBoardProps};
pub use edit::{EditField, EditForm, EditFormProps, EditResult, extract_body_for_edit};
pub use search::{FilteredTicket, filter_tickets};
pub use state::TuiState;
pub use theme::Theme;
pub use view::{IssueBrowser, IssueBrowserProps};
