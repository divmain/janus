//! Local view operations (push, unlink)

use iocraft::prelude::KeyCode;

use super::super::confirm_modal::ConfirmDialogState;
use super::super::error_toast::Toast;
use super::HandleResult;
use super::context::HandlerContext;

/// Handle local view specific operations
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Char('p') => {
            handle_push(ctx);
            HandleResult::Handled
        }
        KeyCode::Char('u') => {
            handle_unlink(ctx);
            HandleResult::Handled
        }
        _ => HandleResult::NotHandled,
    }
}

fn handle_push(ctx: &mut HandlerContext<'_>) {
    let selected_ids: Vec<String> = ctx
        .view_data
        .local_nav
        .selected_ids
        .read()
        .iter()
        .cloned()
        .collect();
    let tickets_to_push = if !selected_ids.is_empty() {
        selected_ids
    } else {
        // Push current item if no selection
        let tickets = ctx.view_data.local_tickets.read();
        if let Some(ticket) = tickets.get(ctx.view_data.local_nav.selected_index.get()) {
            if let Some(id) = &ticket.id {
                vec![id.clone()]
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    };

    if tickets_to_push.is_empty() {
        ctx.modals
            .toast
            .set(Some(Toast::warning("No ticket selected to push")));
        return;
    }

    // Check if any tickets are already linked
    let tickets_ref = ctx.view_data.local_tickets.read();
    let already_linked: Vec<String> = tickets_to_push
        .iter()
        .filter(|id| {
            tickets_ref
                .iter()
                .any(|t| t.id.as_ref() == Some(*id) && t.remote.is_some())
        })
        .cloned()
        .collect();
    drop(tickets_ref);

    if !already_linked.is_empty() {
        ctx.modals.toast.set(Some(Toast::warning(format!(
            "{} ticket(s) already linked, skipping",
            already_linked.len()
        ))));
        // Filter out already linked tickets
        let unlinked: Vec<String> = tickets_to_push
            .into_iter()
            .filter(|id| !already_linked.contains(id))
            .collect();
        if !unlinked.is_empty() {
            ctx.modals.toast.set(Some(Toast::info(format!(
                "Pushing {} ticket(s)...",
                unlinked.len()
            ))));
            let current_query = ctx.filters.active_filters.read().clone();
            ctx.handlers.push_handler.clone()((
                unlinked,
                ctx.filters.provider.get(),
                current_query,
            ));
        }
    } else {
        ctx.modals.toast.set(Some(Toast::info(format!(
            "Pushing {} ticket(s)...",
            tickets_to_push.len()
        ))));
        let current_query = ctx.filters.active_filters.read().clone();
        ctx.handlers.push_handler.clone()((
            tickets_to_push,
            ctx.filters.provider.get(),
            current_query,
        ));
    }
}

fn handle_unlink(ctx: &mut HandlerContext<'_>) {
    let selected_ids: Vec<String> = ctx
        .view_data
        .local_nav
        .selected_ids
        .read()
        .iter()
        .cloned()
        .collect();

    let tickets_to_unlink = if !selected_ids.is_empty() {
        // Filter to only tickets that are actually linked
        let tickets_ref = ctx.view_data.local_tickets.read();
        let linked: Vec<String> = selected_ids
            .into_iter()
            .filter(|id| {
                tickets_ref
                    .iter()
                    .any(|t| t.id.as_ref() == Some(id) && t.remote.is_some())
            })
            .collect();
        drop(tickets_ref);
        linked
    } else {
        // Unlink current item if it's linked
        let tickets = ctx.view_data.local_tickets.read();
        if let Some(ticket) = tickets.get(ctx.view_data.local_nav.selected_index.get())
            && let Some(id) = &ticket.id
            && ticket.remote.is_some()
        {
            vec![id.clone()]
        } else {
            vec![]
        }
    };

    if tickets_to_unlink.is_empty() {
        ctx.modals
            .toast
            .set(Some(Toast::warning("No linked tickets selected")));
        return;
    }

    // Show confirmation dialog instead of executing immediately
    ctx.modals
        .confirm_dialog
        .set(Some(ConfirmDialogState::for_unlink(tickets_to_unlink)));
}
