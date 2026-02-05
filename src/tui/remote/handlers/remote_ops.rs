//! Remote view operations (adopt)

use std::collections::HashSet;

use iocraft::prelude::KeyCode;

use crate::ticket::get_all_tickets_from_disk;

use super::super::error_toast::Toast;
use super::super::operations;

use super::context::HandlerContext;
use super::HandleResult;

/// Handle remote view specific operations
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Char('a') => {
            // Only handle 'a' for adopt when NOT in sync preview
            if ctx.modals.sync_preview.read().is_none() {
                handle_adopt(ctx);
                return HandleResult::Handled;
            }
            HandleResult::NotHandled
        }
        _ => HandleResult::NotHandled,
    }
}

fn handle_adopt(ctx: &mut HandlerContext<'_>) {
    let selected_ids: Vec<String> = ctx
        .view_data
        .remote_nav
        .selected_ids()
        .iter()
        .cloned()
        .collect();

    let issues: Vec<_> = if selected_ids.is_empty() {
        let issues = ctx.view_data.remote_issues.read();
        let selected_idx = ctx.view_data.remote_nav.selected_index();
        if let Some(issue) = issues.get(selected_idx).cloned() {
            vec![issue]
        } else {
            ctx.modals
                .toast
                .set(Some(Toast::error("No issue to adopt")));
            return;
        }
    } else {
        ctx.view_data
            .remote_issues
            .read()
            .iter()
            .filter(|i| selected_ids.contains(&i.id))
            .cloned()
            .collect()
    };

    match operations::adopt_issues(&issues, &ctx.view_data.local_nav.selected_ids()) {
        Ok(ids) => {
            ctx.modals
                .toast
                .set(Some(Toast::info(format!("Adopted {} issues", ids.len()))));
            ctx.view_data
                .local_tickets
                .set(get_all_tickets_from_disk().items);
            ctx.view_data.remote_nav.set_selected_ids(HashSet::new());
        }
        Err(e) => {
            ctx.modals
                .toast
                .set(Some(Toast::error(format!("Adopt failed: {e}"))));
        }
    }
}
