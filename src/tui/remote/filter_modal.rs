//! Filter modal for remote TUI
//!
//! Provides pagination controls for remote issue listing.
//! Note: Server-side filtering is not supported by remote providers.
//! All filtering must be done client-side after fetching results.

use iocraft::prelude::*;

use crate::remote::RemoteQuery;
use crate::tui::components::{
    Clickable, ModalBorderColor, ModalContainer, ModalHeight, ModalOverlay, ModalWidth,
};

/// Filter modal state
#[derive(Debug, Clone, Default)]
pub struct FilterState {
    /// Page size limit
    pub limit: u32,
}

impl FilterState {
    /// Create a new filter state from an existing query
    pub fn from_query(query: &RemoteQuery) -> Self {
        Self { limit: query.limit }
    }

    /// Convert to a RemoteQuery
    pub fn to_query(&self, base: &RemoteQuery) -> RemoteQuery {
        let mut query = base.clone();
        query.limit = self.limit;
        query
    }

    /// Increase limit
    pub fn increase_limit(&mut self) {
        self.limit = (self.limit + 10).min(100);
    }

    /// Decrease limit
    pub fn decrease_limit(&mut self) {
        self.limit = self.limit.saturating_sub(10).max(10);
    }

    /// Check if any filter is active (always false now since we only have limit)
    pub fn has_active_filters(&self) -> bool {
        false
    }

    /// Reset to defaults
    pub fn clear(&mut self) {
        self.limit = 100;
    }
}

/// Props for the filter modal
#[derive(Default, Props)]
pub struct FilterModalProps {
    pub state: FilterState,
    /// Handler invoked when modal is closed via X button
    pub on_close: Option<Handler<()>>,
    /// Handler invoked when limit field is clicked
    pub on_limit_click: Option<Handler<()>>,
}

/// Filter modal component
#[component]
pub fn FilterModal<'a>(props: &FilterModalProps, _hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let state = &props.state;

    let limit_focused = true;

    element! {
        ModalOverlay() {
            ModalContainer(
                width: Some(ModalWidth::Fixed(60)),
                height: Some(ModalHeight::Fixed(10)),
                border_color: Some(ModalBorderColor::Info),
                title: Some("Remote Query Settings".to_string()),
                footer_text: Some("+/-: adjust | r: reset | Enter/Esc: close".to_string()),
                on_close: props.on_close.clone(),
            ) {
                Text(content: "Note: Server-side filtering is not supported.")
                Text(content: "Use client-side search (/) after fetching results.")
                Text(content: "")

                // Limit field - clickable to focus
                Clickable(
                    on_click: props.on_limit_click.clone(),
                ) {
                    View(
                        width: 100pct,
                        flex_direction: FlexDirection::Row,
                        background_color: if limit_focused { Some(Color::DarkBlue) } else { None },
                    ) {
                        Text(
                            content: "Page Size: ",
                            color: if limit_focused { Color::Yellow } else { Color::White },
                        )
                        Text(
                            content: format!("[{}]", state.limit),
                            color: Color::Cyan,
                        )
                        Text(
                            content: if limit_focused { " (+/- to adjust)" } else { "" },
                            color: Color::DarkGrey,
                        )
                    }
                }
            }
        }
    }
}
