//! Filter modal mode handlers

use iocraft::prelude::KeyCode;

use super::super::error_toast::Toast;
use super::HandleResult;
use super::context::HandlerContext;

/// Handle filter modal events
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Tab => {
            let state = ctx.filter_state.read().clone();
            if let Some(mut s) = state {
                s.focus_next();
                ctx.filter_state.set(Some(s));
                HandleResult::Handled
            } else {
                HandleResult::NotHandled
            }
        }
        KeyCode::BackTab => {
            let state = ctx.filter_state.read().clone();
            if let Some(mut s) = state {
                s.focus_prev();
                ctx.filter_state.set(Some(s));
                HandleResult::Handled
            } else {
                HandleResult::NotHandled
            }
        }
        KeyCode::Char('x') => {
            let state = ctx.filter_state.read().clone();
            if let Some(mut s) = state {
                s.clear();
                ctx.filter_state.set(Some(s));
                HandleResult::Handled
            } else {
                HandleResult::NotHandled
            }
        }
        KeyCode::Enter => {
            let state = ctx.filter_state.read().clone();
            if let Some(s) = state {
                if s.focused_field == 0 {
                    let mut new_state = s;
                    new_state.toggle_status();
                    ctx.filter_state.set(Some(new_state));
                } else {
                    let base_query = ctx.active_filters.read().clone();
                    let new_query = s.to_query(&base_query);
                    ctx.active_filters.set(new_query.clone());
                    ctx.filter_state.set(None);
                    ctx.remote_loading.set(true);
                    ctx.toast.set(Some(Toast::info("Applying filters...")));
                    ctx.fetch_handler.clone()((ctx.provider.get(), new_query));
                }
                HandleResult::Handled
            } else {
                HandleResult::NotHandled
            }
        }
        KeyCode::Char('j') | KeyCode::Down => {
            let state = ctx.filter_state.read().clone();
            if let Some(mut s) = state {
                s.focus_next();
                ctx.filter_state.set(Some(s));
                HandleResult::Handled
            } else {
                HandleResult::NotHandled
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let state = ctx.filter_state.read().clone();
            if let Some(mut s) = state {
                s.focus_prev();
                ctx.filter_state.set(Some(s));
                HandleResult::Handled
            } else {
                HandleResult::NotHandled
            }
        }
        _ => HandleResult::NotHandled,
    }
}
