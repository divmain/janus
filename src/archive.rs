//! Auto-archive sweep for completed tickets.
//!
//! When a ticket has been in `Complete` status longer than the configured
//! threshold (`archive.days`), the sweep transitions it to `Archived`. The
//! threshold is measured against `completed-at` when present, falling back to
//! the file's mtime when the ticket predates that field.

use std::path::Path;
use std::time::{Duration, SystemTime};

use jiff::Timestamp;

use crate::config::Config;
use crate::error::Result;
use crate::ticket::Ticket;
use crate::types::{TicketMetadata, TicketStatus};

/// Result of running the archive sweep.
#[derive(Debug, Default, Clone)]
pub struct ArchiveResult {
    /// Ticket IDs that were moved from Complete to Archived.
    pub archived_ids: Vec<String>,
    /// Per-ticket errors encountered during the sweep. Failures are logged but
    /// don't abort the sweep — one bad ticket shouldn't block the rest.
    pub errors: Vec<(String, String)>,
}

impl ArchiveResult {
    pub fn count(&self) -> usize {
        self.archived_ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.archived_ids.is_empty() && self.errors.is_empty()
    }
}

/// Archive any Complete tickets older than the configured threshold.
///
/// Returns the list of ticket IDs that were archived. If auto-archive is
/// disabled (`archive.days = 0`) or the config cannot be loaded, returns an
/// empty result without error so callers can invoke this unconditionally at
/// startup.
pub async fn sweep_completed_tickets(tickets: &[TicketMetadata]) -> Result<ArchiveResult> {
    let config = Config::load().unwrap_or_default();
    let Some(threshold) = config.archive.threshold() else {
        return Ok(ArchiveResult::default());
    };

    sweep_with_threshold(tickets, threshold).await
}

/// Run the sweep with an explicit threshold. Separated from `sweep_completed_tickets`
/// so tests can pass a short duration without touching the config file.
pub async fn sweep_with_threshold(
    tickets: &[TicketMetadata],
    threshold: Duration,
) -> Result<ArchiveResult> {
    let now = SystemTime::now();
    let mut result = ArchiveResult::default();

    for ticket in tickets {
        if ticket.status != Some(TicketStatus::Complete) {
            continue;
        }
        let Some(id) = ticket.id.as_ref() else {
            continue;
        };
        let age = match ticket_age(ticket, now) {
            Some(age) => age,
            None => continue,
        };
        if age < threshold {
            continue;
        }

        let id_str = id.to_string();
        match Ticket::find(&id_str).await {
            Ok(t) => match t.update_status(TicketStatus::Archived, None) {
                Ok(()) => result.archived_ids.push(id_str),
                Err(e) => result.errors.push((id_str, e.to_string())),
            },
            Err(e) => result.errors.push((id_str, e.to_string())),
        }
    }

    Ok(result)
}

/// Compute the age of a ticket's completion, preferring `completed-at` and
/// falling back to the file's mtime when unavailable. Returns `None` when
/// neither signal is usable (e.g., ticket has no file path and no timestamp).
pub fn ticket_age(ticket: &TicketMetadata, now: SystemTime) -> Option<Duration> {
    if let Some(stamp) = ticket
        .completed_at
        .as_ref()
        .and_then(|c| c.to_timestamp())
    {
        return duration_between(stamp, now);
    }
    ticket
        .file_path
        .as_deref()
        .and_then(|p| file_mtime_age(p, now))
}

fn duration_between(stamp: Timestamp, now: SystemTime) -> Option<Duration> {
    // `as_nanosecond()` returns i128 since UNIX_EPOCH; we need to preserve full
    // precision when constructing a SystemTime, because i128 values easily
    // overflow u64 nanoseconds for post-1970 timestamps once we reach ~584 years
    // worth of ns. Split into seconds + sub-second ns to stay safe.
    let total_ns = stamp.as_nanosecond();
    if total_ns < 0 {
        return None;
    }
    let secs = u64::try_from(total_ns / 1_000_000_000).ok()?;
    let nanos = (total_ns % 1_000_000_000) as u32;
    let stamp_system = SystemTime::UNIX_EPOCH + Duration::new(secs, nanos);
    now.duration_since(stamp_system).ok()
}

fn file_mtime_age(path: &Path, now: SystemTime) -> Option<Duration> {
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    now.duration_since(mtime).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CreatedAt, TicketId};

    fn make_ticket(id: &str, status: TicketStatus, completed_at: Option<&str>) -> TicketMetadata {
        TicketMetadata {
            id: Some(TicketId::new_unchecked(id)),
            status: Some(status),
            completed_at: completed_at.map(CreatedAt::new_unchecked),
            ..Default::default()
        }
    }

    // Anchor used across the age tests. 1700000000 UTC = 2023-11-14T22:13:20Z.
    const ANCHOR_UNIX: u64 = 1_700_000_000;
    const ANCHOR_ISO: &str = "2023-11-14T22:13:20Z";

    #[test]
    fn test_ticket_age_prefers_completed_at() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(ANCHOR_UNIX);
        let ticket = make_ticket("j-a1b2", TicketStatus::Complete, Some(ANCHOR_ISO));
        let age = ticket_age(&ticket, now).unwrap();
        assert_eq!(age, Duration::ZERO);
    }

    #[test]
    fn test_ticket_age_missing_both_returns_none() {
        let now = SystemTime::now();
        let ticket = make_ticket("j-a1b2", TicketStatus::Complete, None);
        assert!(ticket_age(&ticket, now).is_none());
    }

    #[test]
    fn test_duration_between_returns_zero_for_equal_timestamps() {
        let stamp: Timestamp = ANCHOR_ISO.parse().unwrap();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(ANCHOR_UNIX);
        assert_eq!(duration_between(stamp, now).unwrap(), Duration::ZERO);
    }

    #[test]
    fn test_sweep_result_empty_initially() {
        let r = ArchiveResult::default();
        assert!(r.is_empty());
        assert_eq!(r.count(), 0);
    }
}
