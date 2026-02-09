//! Selection handler (Space key for toggling selection)

use iocraft::prelude::KeyCode;

use super::super::state::ViewMode;
use super::context::HandlerContext;
use super::HandleResult;

/// Handle Space key for toggling selection
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    if code != KeyCode::Char(' ') {
        return HandleResult::NotHandled;
    }

    if ctx.view_state.active_view() == ViewMode::Local {
        toggle_local_selection(ctx);
    } else {
        toggle_remote_selection(ctx);
    }

    HandleResult::Handled
}

fn toggle_local_selection(ctx: &mut HandlerContext<'_>) {
    let tickets = ctx.view_data.local_tickets.read();
    if let Some(ticket) = tickets.get(ctx.view_data.local_nav.selected_index())
        && let Some(id) = &ticket.id
    {
        let id_str = id.to_string();
        drop(tickets);
        let mut ids = ctx.view_data.local_nav.selected_ids();
        if ids.contains(&id_str) {
            ids.remove(&id_str);
        } else {
            ids.insert(id_str);
        }
        ctx.view_data.local_nav.set_selected_ids(ids);
    }
}

fn toggle_remote_selection(ctx: &mut HandlerContext<'_>) {
    let issues = ctx.view_data.remote_issues.read();
    if let Some(issue) = issues.get(ctx.view_data.remote_nav.selected_index()) {
        let id = issue.id.clone();
        drop(issues);
        let mut ids = ctx.view_data.remote_nav.selected_ids();
        if ids.contains(&id) {
            ids.remove(&id);
        } else {
            ids.insert(id);
        }
        ctx.view_data.remote_nav.set_selected_ids(ids);
    }
}
