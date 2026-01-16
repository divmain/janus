//! Remote view operations (adopt)

use std::collections::HashSet;

use iocraft::prelude::KeyCode;

use crate::ticket::get_all_tickets_from_disk;

use super::super::error_toast::Toast;
use super::super::operations;
use super::HandleResult;
use super::context::HandlerContext;

/// Handle remote view specific operations
pub fn handle(ctx: &mut HandlerContext<'_>, code: KeyCode) -> HandleResult {
    match code {
        KeyCode::Char('a') => {
            // Only handle 'a' for adopt when NOT in sync preview
            if ctx.sync_preview.read().is_none() {
                handle_adopt(ctx);
                return HandleResult::Handled;
            }
            HandleResult::NotHandled
        }
        _ => HandleResult::NotHandled,
    }
}

fn handle_adopt(ctx: &mut HandlerContext<'_>) {
    let selected_ids: Vec<String> = ctx.remote_selected_ids.read().iter().cloned().collect();
    if selected_ids.is_empty() {
        return;
    }

    let issues: Vec<_> = ctx
        .remote_issues
        .read()
        .iter()
        .filter(|i| selected_ids.contains(&i.id))
        .cloned()
        .collect();

    match operations::adopt_issues(&issues, &ctx.local_selected_ids.read()) {
        Ok(ids) => {
            ctx.toast
                .set(Some(Toast::info(format!("Adopted {} issues", ids.len()))));
            ctx.local_tickets.set(get_all_tickets_from_disk());
            ctx.remote_selected_ids.set(HashSet::new());
        }
        Err(e) => {
            ctx.toast
                .set(Some(Toast::error(format!("Adopt failed: {}", e))));
        }
    }
}
