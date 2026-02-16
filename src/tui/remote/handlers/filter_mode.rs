//! Filter modal mode handlers

use iocraft::prelude::KeyCode;

use super::super::error_toast::Toast;
use super::HandleResult;
use super::context::HandlerContext;

/// Handle filter modal events
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        // Tab and BackTab are no-ops since there's only one field
        KeyCode::Tab | KeyCode::BackTab => HandleResult::Handled,
        KeyCode::Char('r') => {
            let state = ctx.filters.filter_modal.read().clone();
            if let Some(mut s) = state {
                s.clear();
                ctx.filters.filter_modal.set(Some(s));
                HandleResult::Handled
            } else {
                HandleResult::NotHandled
            }
        }
        KeyCode::Enter => {
            let state = ctx.filters.filter_modal.read().clone();
            if let Some(s) = state {
                let base_query = ctx.filters.active_filters();
                let new_query = s.to_query(&base_query);
                ctx.filters.set_active_filters(new_query.clone());
                ctx.filters.filter_modal.set(None);
                ctx.view_state.set_loading(true);
                ctx.modals
                    .toast
                    .set(Some(Toast::info("Applying settings...")));
                ctx.handlers.fetch_handler.clone()((ctx.filters.provider(), new_query));
                HandleResult::Handled
            } else {
                HandleResult::NotHandled
            }
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            let state = ctx.filters.filter_modal.read().clone();
            if let Some(mut s) = state {
                s.increase_limit();
                ctx.filters.filter_modal.set(Some(s));
                HandleResult::Handled
            } else {
                HandleResult::NotHandled
            }
        }
        KeyCode::Char('-') => {
            let state = ctx.filters.filter_modal.read().clone();
            if let Some(mut s) = state {
                s.decrease_limit();
                ctx.filters.filter_modal.set(Some(s));
                HandleResult::Handled
            } else {
                HandleResult::NotHandled
            }
        }
        _ => HandleResult::NotHandled,
    }
}
