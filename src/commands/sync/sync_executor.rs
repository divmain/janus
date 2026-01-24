use crate::error::Result;
use crate::remote::config::Config;
use crate::remote::{RemoteProvider, RemoteRef, create_provider};
use crate::ticket::Ticket;

use super::sync_ui::SyncDecision;

pub async fn apply_sync_changes(
    decisions: &[SyncDecision],
    ticket: &Ticket,
    remote_ref: &RemoteRef,
    config: &Config,
) -> Result<()> {
    let mut remote_updates_to_apply: Option<crate::remote::IssueUpdates> = None;

    for decision in decisions {
        match decision {
            SyncDecision::UpdateLocal { field, value } => {
                ticket.update_field(field, value)?;
            }
            SyncDecision::UpdateRemote(updates) => {
                if remote_updates_to_apply.is_none() {
                    remote_updates_to_apply = Some(crate::remote::IssueUpdates::default());
                }
                let remote_updates = &mut remote_updates_to_apply.as_mut().unwrap();
                if updates.title.is_some() {
                    remote_updates.title = updates.title.clone();
                }
                if updates.status.is_some() {
                    remote_updates.status = updates.status.clone();
                }
            }
            SyncDecision::UpdateLocalTitle { new_content } => {
                ticket.write(new_content)?;
            }
            SyncDecision::Skip => {}
        }
    }

    if let Some(remote_updates) = remote_updates_to_apply {
        let provider = create_provider(&remote_ref.platform(), config)?;
        provider.update_issue(remote_ref, remote_updates).await?;
    }

    Ok(())
}
