//! Global keyboard handlers (q, /, P, r, f, ?, e, Enter, Tab, etc.)

use iocraft::prelude::KeyCode;

use super::super::error_toast::Toast;
use super::super::filter_modal::FilterState;
use super::context::HandlerContext;
use super::HandleResult;

/// Handle global keys that work in most contexts
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => {
            ctx.view_state.set_should_exit(true);
            HandleResult::Handled
        }
        KeyCode::Char('/') => {
            ctx.search.set_focused(true);
            HandleResult::Handled
        }
        KeyCode::Char('P') => {
            handle_switch_provider(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('r') => {
            handle_refresh(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('f') => {
            handle_filter(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('?') => {
            ctx.modals.toggle_help();
            HandleResult::Handled
        }
        KeyCode::Char('e') => {
            ctx.modals.toggle_error();
            HandleResult::Handled
        }
        KeyCode::Enter => {
            handle_enter(ctx);
            HandleResult::Handled
        }
        KeyCode::Tab => {
            handle_tab(ctx);
            HandleResult::Handled
        }
        KeyCode::BackTab => {
            handle_backtab(ctx);
            HandleResult::Handled
        }
        _ => HandleResult::NotHandled,
    }
}

/// Handle 'P' key - switch provider
fn handle_switch_provider(ctx: &mut HandlerContext<'_>) {
    let current = ctx.filters.provider();
    let new_provider = match current {
        crate::remote::config::Platform::GitHub => crate::remote::config::Platform::Linear,
        crate::remote::config::Platform::Linear => crate::remote::config::Platform::GitHub,
    };
    ctx.filters.set_provider(new_provider);
    ctx.modals
        .toast
        .set(Some(Toast::info(format!("Switched to {}", new_provider))));
}

/// Handle 'r' key - refresh
fn handle_refresh(ctx: &mut HandlerContext<'_>) {
    ctx.view_state.set_loading(true);
    let current_provider = ctx.filters.provider();
    let current_query = ctx.filters.active_filters();
    ctx.handlers.fetch_handler.clone()((current_provider, current_query));
}

/// Handle 'f' key - open filter modal
fn handle_filter(ctx: &mut HandlerContext<'_>) {
    let current_query = ctx.filters.active_filters();
    ctx.filters
        .filter_modal
        .set(Some(FilterState::from_query(&current_query)));
}

/// Handle Enter key - depends on context
fn handle_enter(ctx: &mut HandlerContext<'_>) {
    if ctx.search.is_focused() {
        // Execute search
        ctx.search.set_focused(false);
    } else {
        // Toggle detail view
        ctx.view_state.toggle_show_detail();
    }
}

/// Handle Tab key - toggle view or focus
fn handle_tab(ctx: &mut HandlerContext<'_>) {
    if ctx.view_state.show_detail() {
        // Toggle between detail pane and list
        ctx.view_state
            .set_detail_pane_focused(!ctx.view_state.detail_pane_focused());
    } else {
        // Toggle between local and remote views
        ctx.view_state.toggle_view();
    }
}

/// Handle BackTab key
fn handle_backtab(ctx: &mut HandlerContext<'_>) {
    // Same as Tab for now
    handle_tab(ctx);
}
