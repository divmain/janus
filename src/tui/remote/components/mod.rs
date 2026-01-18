//! Sub-components for the remote TUI
//!
//! This module contains focused rendering components broken out from
//! the main view.rs to improve maintainability.

mod detail_pane;
mod header;
mod list_pane;
pub mod overlays;
mod selection_bar;
mod tab_bar;

pub use detail_pane::DetailPane;
pub use header::RemoteHeader;
pub use list_pane::ListPane;
pub use overlays::ModalOverlays;
pub use selection_bar::SelectionBar;
pub use tab_bar::TabBar;
