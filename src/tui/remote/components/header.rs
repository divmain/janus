//! Remote TUI header component
//!
//! Displays the "janus remote [Provider]" header with help indicator.

use iocraft::prelude::*;

use crate::remote::config::Platform;
use crate::tui::theme::theme;

/// Props for the RemoteHeader component
#[derive(Default, Props)]
pub struct RemoteHeaderProps {
    /// The current remote provider (GitHub or Linear)
    pub provider: Option<Platform>,
}

/// Header row showing "janus remote [Provider]" with help indicator
#[component]
pub fn RemoteHeader(props: &RemoteHeaderProps) -> impl Into<AnyElement<'static>> {
    let theme = theme();
    let provider = props.provider.unwrap_or(Platform::GitHub);

    element! {
        View(
            width: 100pct,
            padding_left: 1,
            padding_right: 1,
        ) {
            Text(
                content: "janus remote",
                color: Color::Cyan,
                weight: Weight::Bold,
            )
            Text(
                content: format!(" [{}]", provider),
                color: theme.text_dimmed,
            )
            View(flex_grow: 1.0)
            Text(content: "[?]", color: theme.text_dimmed)
        }
    }
}
