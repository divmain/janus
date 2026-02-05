//! Link mode handlers

use iocraft::prelude::KeyCode;

use super::super::error_toast::Toast;
use super::super::link_mode::{LinkModeState, LinkSource};
use super::super::state::ViewMode;
use super::context::HandlerContext;
use super::HandleResult;

/// Handle link mode completion (when link mode is active and 'l' is pressed)
pub fn handle_link_mode(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    if code != KeyCode::Char('l') {
        return HandleResult::NotHandled;
    }

    // Complete link operation
    let Some(lm) = ctx.modals.link_mode.read().clone() else {
        return HandleResult::NotHandled;
    };
    if lm.source_view == ViewMode::Local {
        // Source is local ticket, target is remote issue
        let issues = ctx.view_data.remote_issues.read();
        if let Some(issue) = issues
            .get(ctx.view_data.remote_nav.selected_index())
            .cloned()
        {
            drop(issues);
            // Queue the link operation to be executed asynchronously
            ctx.modals.toast.set(Some(Toast::info(format!(
                "Linking {} to {}...",
                lm.source_id, issue.id
            ))));
            ctx.handlers.link_handler.clone()(LinkSource {
                ticket_id: lm.source_id.clone(),
                remote_issue: issue,
            });
        }
    } else {
        // Source is remote issue, target is local ticket
        let tickets = ctx.view_data.local_tickets.read();
        if let Some(ticket) = tickets
            .get(ctx.view_data.local_nav.selected_index())
            .cloned()
        {
            drop(tickets);
            if let Some(ticket_id) = &ticket.id {
                // Find the source remote issue
                let issues = ctx.view_data.remote_issues.read();
                if let Some(source_issue) = issues.iter().find(|i| i.id == lm.source_id).cloned() {
                    drop(issues);
                    // Queue the link operation to be executed asynchronously
                    ctx.modals.toast.set(Some(Toast::info(format!(
                        "Linking {} to {}...",
                        ticket_id, source_issue.id
                    ))));
                    ctx.handlers.link_handler.clone()(LinkSource {
                        ticket_id: ticket_id.clone(),
                        remote_issue: source_issue,
                    });
                }
            }
        }
    }

    ctx.modals.link_mode.set(None);
    ctx.view_state.set_active_view(lm.source_view);
    HandleResult::Handled
}

/// Handle starting link mode (when 'l' is pressed and link mode is not active)
pub fn handle_link_start(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    if code != KeyCode::Char('l') {
        return HandleResult::NotHandled;
    }

    // Link mode is not active, so start it
    if ctx.view_state.active_view() == ViewMode::Local {
        let tickets = ctx.view_data.local_tickets.read();
        if let Some(ticket) = tickets.get(ctx.view_data.local_nav.selected_index())
            && let Some(id) = &ticket.id
        {
            let title = ticket.title.as_deref().unwrap_or("").to_string();
            let id_clone = id.clone();
            drop(tickets);
            ctx.modals
                .link_mode
                .set(Some(LinkModeState::new(ViewMode::Local, id_clone, title)));
            ctx.view_state.set_active_view(ViewMode::Remote);
        }
    } else {
        let issues = ctx.view_data.remote_issues.read();
        if let Some(issue) = issues.get(ctx.view_data.remote_nav.selected_index()) {
            let lm = LinkModeState::new(ViewMode::Remote, issue.id.clone(), issue.title.clone());
            drop(issues);
            ctx.modals.link_mode.set(Some(lm));
            ctx.view_state.set_active_view(ViewMode::Local);
        }
    }

    HandleResult::Handled
}
