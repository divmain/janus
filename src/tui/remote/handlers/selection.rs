//! Selection handler (Space key for toggling selection)

use iocraft::prelude::KeyCode;

use super::super::state::ViewMode;
use super::HandleResult;
use super::context::HandlerContext;

/// Handle Space key for toggling selection
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    if code != KeyCode::Char(' ') {
        return HandleResult::NotHandled;
    }

    if ctx.active_view.get() == ViewMode::Local {
        toggle_local_selection(ctx);
    } else {
        toggle_remote_selection(ctx);
    }

    HandleResult::Handled
}

fn toggle_local_selection(ctx: &mut HandlerContext<'_>) {
    let tickets = ctx.local_tickets.read();
    if let Some(ticket) = tickets.get(ctx.local_selected_index.get())
        && let Some(id) = &ticket.id
    {
        let id = id.clone();
        drop(tickets);
        let mut ids = ctx.local_selected_ids.read().clone();
        if ids.contains(&id) {
            ids.remove(&id);
        } else {
            ids.insert(id);
        }
        ctx.local_selected_ids.set(ids);
    }
}

fn toggle_remote_selection(ctx: &mut HandlerContext<'_>) {
    let issues = ctx.remote_issues.read();
    if let Some(issue) = issues.get(ctx.remote_selected_index.get()) {
        let id = issue.id.clone();
        drop(issues);
        let mut ids = ctx.remote_selected_ids.read().clone();
        if ids.contains(&id) {
            ids.remove(&id);
        } else {
            ids.insert(id);
        }
        ctx.remote_selected_ids.set(ids);
    }
}
