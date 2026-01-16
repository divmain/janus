//! Filter modal mode handlers

use iocraft::prelude::KeyCode;

use super::super::error_toast::Toast;
use super::HandleResult;
use super::context::HandlerContext;

/// Handle filter modal events
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Tab => {
            let mut state = ctx.filter_state.read().clone().unwrap();
            state.focus_next();
            ctx.filter_state.set(Some(state));
            HandleResult::Handled
        }
        KeyCode::BackTab => {
            let mut state = ctx.filter_state.read().clone().unwrap();
            state.focus_prev();
            ctx.filter_state.set(Some(state));
            HandleResult::Handled
        }
        KeyCode::Char('x') => {
            let mut state = ctx.filter_state.read().clone().unwrap();
            state.clear();
            ctx.filter_state.set(Some(state));
            HandleResult::Handled
        }
        KeyCode::Enter => {
            let state = ctx.filter_state.read().clone().unwrap();
            if state.focused_field == 0 {
                // Toggle status
                let mut new_state = state.clone();
                new_state.toggle_status();
                ctx.filter_state.set(Some(new_state));
            } else {
                // Apply filters
                let base_query = ctx.active_filters.read().clone();
                let new_query = state.to_query(&base_query);
                ctx.active_filters.set(new_query.clone());
                ctx.filter_state.set(None);
                // Refresh with new filters
                ctx.remote_loading.set(true);
                ctx.toast.set(Some(Toast::info("Applying filters...")));
                ctx.fetch_handler.clone()((ctx.provider.get(), new_query));
            }
            HandleResult::Handled
        }
        KeyCode::Char('j') | KeyCode::Down => {
            let mut state = ctx.filter_state.read().clone().unwrap();
            state.focus_next();
            ctx.filter_state.set(Some(state));
            HandleResult::Handled
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let mut state = ctx.filter_state.read().clone().unwrap();
            state.focus_prev();
            ctx.filter_state.set(Some(state));
            HandleResult::Handled
        }
        _ => HandleResult::NotHandled,
    }
}
