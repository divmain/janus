//! Local view operations (push, unlink)

use iocraft::prelude::KeyCode;
use std::collections::HashSet;

use super::super::confirm_modal::ConfirmDialogState;
use super::super::error_toast::Toast;
use super::context::HandlerContext;
use super::HandleResult;

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
        .selected_ids()
        .iter()
        .cloned()
        .collect();
    let tickets_to_push = if !selected_ids.is_empty() {
        selected_ids
    } else {
        // Push current item if no selection
        let tickets = ctx.view_data.local_tickets.read();
        if let Some(ticket) = tickets.get(ctx.view_data.local_nav.selected_index()) {
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
    let linked_ids: HashSet<String> = tickets_ref
        .iter()
        .filter(|t| t.remote.is_some())
        .filter_map(|t| t.id.clone())
        .collect();
    drop(tickets_ref);

    let already_linked: Vec<String> = tickets_to_push
        .iter()
        .filter(|id| linked_ids.contains(id.as_str()))
        .cloned()
        .collect();

    if !already_linked.is_empty() {
        ctx.modals.toast.set(Some(Toast::warning(format!(
            "{} ticket(s) already linked, skipping",
            already_linked.len()
        ))));
        // Filter out already linked tickets
        let unlinked: Vec<String> = tickets_to_push
            .into_iter()
            .filter(|id| !linked_ids.contains(id.as_str()))
            .collect();
        if !unlinked.is_empty() {
            ctx.modals.toast.set(Some(Toast::info(format!(
                "Pushing {} ticket(s)...",
                unlinked.len()
            ))));
            let current_query = ctx.filters.active_filters();
            ctx.handlers.push_handler.clone()((unlinked, ctx.filters.provider(), current_query));
        }
    } else {
        ctx.modals.toast.set(Some(Toast::info(format!(
            "Pushing {} ticket(s)...",
            tickets_to_push.len()
        ))));
        let current_query = ctx.filters.active_filters();
        ctx.handlers.push_handler.clone()((tickets_to_push, ctx.filters.provider(), current_query));
    }
}

fn handle_unlink(ctx: &mut HandlerContext<'_>) {
    let selected_ids: Vec<String> = ctx
        .view_data
        .local_nav
        .selected_ids()
        .iter()
        .cloned()
        .collect();

    let tickets_to_unlink = if !selected_ids.is_empty() {
        // Filter to only tickets that are actually linked
        let tickets_ref = ctx.view_data.local_tickets.read();
        let linked_ids: HashSet<String> = tickets_ref
            .iter()
            .filter(|t| t.remote.is_some())
            .filter_map(|t| t.id.clone())
            .collect();
        let linked: Vec<String> = selected_ids
            .into_iter()
            .filter(|id| linked_ids.contains(id.as_str()))
            .collect();
        drop(tickets_ref);
        linked
    } else {
        // Unlink current item if it's linked
        let tickets = ctx.view_data.local_tickets.read();
        if let Some(ticket) = tickets.get(ctx.view_data.local_nav.selected_index())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TicketStatus, TicketType};

    fn create_test_ticket(id: &str, remote: Option<&str>) -> crate::types::TicketMetadata {
        crate::types::TicketMetadata {
            id: Some(id.to_string()),
            remote: remote.map(|s| s.to_string()),
            status: Some(TicketStatus::New),
            ticket_type: Some(TicketType::Task),
            priority: Some(crate::types::TicketPriority::P2),
            title: Some(format!("Ticket {id}")),
            ..Default::default()
        }
    }

    #[test]
    fn test_hashset_linked_ids_optimization() {
        // Test that the HashSet-based optimization correctly identifies linked tickets
        let tickets = vec![
            create_test_ticket("j-1", Some("github:owner/repo/1")),
            create_test_ticket("j-2", None),
            create_test_ticket("j-3", Some("linear:acme/ENG-123")),
        ];

        // Build HashSet using the optimized approach
        let linked_ids: HashSet<String> = tickets
            .iter()
            .filter(|t| t.remote.is_some())
            .filter_map(|t| t.id.clone())
            .collect();

        // VerifyHashSet contains only linked tickets
        assert_eq!(linked_ids.len(), 2);
        assert!(linked_ids.contains("j-1"));
        assert!(linked_ids.contains("j-3"));
        assert!(!linked_ids.contains("j-2"));
    }

    #[test]
    fn test_filter_selected_ids_using_hashset() {
        // Test that filtering selected IDs using HashSet works correctly
        let tickets = vec![
            create_test_ticket("j-1", Some("github:owner/repo/1")),
            create_test_ticket("j-2", None),
            create_test_ticket("j-3", Some("linear:acme/ENG-123")),
        ];

        let selected_ids = [
            "j-1".to_string(),
            "j-2".to_string(),
            "j-3".to_string(),
            "j-4".to_string(),
        ];

        // Build HashSet using the optimized approach
        let linked_ids: HashSet<String> = tickets
            .iter()
            .filter(|t| t.remote.is_some())
            .filter_map(|t| t.id.clone())
            .collect();

        // Filter selected IDs using HashSet
        let already_linked: Vec<String> = selected_ids
            .iter()
            .filter(|id| linked_ids.contains(id.as_str()))
            .cloned()
            .collect();

        assert_eq!(already_linked.len(), 2);
        assert!(already_linked.contains(&String::from("j-1")));
        assert!(already_linked.contains(&String::from("j-3")));
        assert!(!already_linked.contains(&String::from("j-2")));
        assert!(!already_linked.contains(&String::from("j-4")));
    }

    #[test]
    fn test_performance_hashset_vs_iterative() {
        // Test that HashSet approach is correct by comparing results
        let tickets: Vec<_> = (0..100)
            .map(|i| {
                let remote = if i % 2 == 0 {
                    Some(format!("remote:{i}"))
                } else {
                    None
                };
                create_test_ticket(&format!("j-{i}"), remote.as_deref())
            })
            .collect();

        let selected_ids: Vec<_> = (0..50).map(|i| format!("j-{i}")).collect();

        // HashSet approach (optimized)
        let linked_ids: HashSet<String> = tickets
            .iter()
            .filter(|t| t.remote.is_some())
            .filter_map(|t| t.id.clone())
            .collect();

        let already_linked_hash: Vec<String> = selected_ids
            .iter()
            .filter(|id| linked_ids.contains(id.as_str()))
            .cloned()
            .collect();

        // Iterative approach (original)
        let already_linked_iter: Vec<String> = selected_ids
            .iter()
            .filter(|id| {
                tickets
                    .iter()
                    .any(|t| t.id.as_ref() == Some(id) && t.remote.is_some())
            })
            .cloned()
            .collect();

        // Both should produce the same results
        assert_eq!(
            already_linked_hash, already_linked_iter,
            "HashSet and iterative approaches should produce identical results"
        );
    }
}
