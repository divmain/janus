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

    let remote_ticket_status = remote_issue.status.to_ticket_status();
    let status_diff = if local_status != remote_ticket_status {
        Some(StatusDiff {
            local: local_status,
            remote_status: remote_ticket_status,
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
