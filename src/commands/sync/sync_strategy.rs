use crate::remote::RemoteIssue;
use crate::types::TicketStatus;

pub struct SyncPlan {
    pub title_diff: Option<TitleDiff>,
    pub status_diff: Option<StatusDiff>,
}

pub struct TitleDiff {
    pub local: String,
    pub remote: String,
}

pub struct StatusDiff {
    pub local: TicketStatus,
    pub remote_status: TicketStatus,
    pub remote_raw: crate::remote::RemoteStatus,
}

pub fn compute_sync_state(
    local_title: String,
    local_status: TicketStatus,
    remote_issue: &RemoteIssue,
) -> SyncPlan {
    let title_diff = if local_title != remote_issue.title {
        Some(TitleDiff {
            local: local_title,
            remote: remote_issue.title.clone(),
        })
    } else {
        None
    };

    // Use resolve_with_local to avoid lossy round-trip status corruption.
    // This preserves more-specific local statuses (e.g., InProgress, Cancelled)
    // when the remote only has coarse-grained states (Open/Closed).
    let resolved_status = remote_issue.status.resolve_with_local(local_status);
    let status_diff = if local_status != resolved_status {
        Some(StatusDiff {
            local: local_status,
            remote_status: resolved_status,
            remote_raw: remote_issue.status.clone(),
        })
    } else {
        None
    };

    SyncPlan {
        title_diff,
        status_diff,
    }
}
