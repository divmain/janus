pub mod sync_executor;
pub mod sync_strategy;
pub mod sync_ui;

pub use sync_executor::apply_sync_changes;
pub use sync_strategy::{StatusDiff, SyncPlan, TitleDiff, compute_sync_state};
pub use sync_ui::{SyncChoice, SyncDecision, generate_sync_json, prompt_user_for_action};

use owo_colors::OwoColorize;
use serde_json::json;

use super::{CommandOutput, print_json};
use crate::error::{JanusError, Result};
use crate::remote::config::Config;
use crate::remote::{RemoteIssue, RemoteProvider, RemoteRef, create_provider};
use crate::ticket::extract_body;
use crate::ticket::{Ticket, TicketBuilder};
use crate::types::TicketType;

pub async fn cmd_adopt(
    remote_ref_str: &str,
    prefix: Option<&str>,
    output_json: bool,
) -> Result<()> {
    if let Some(p) = prefix {
        crate::utils::validate_prefix(p)?;
    }

    let config = Config::load()?;
    let remote_ref = RemoteRef::parse(remote_ref_str, Some(&config))?;

    let provider = create_provider(&remote_ref.platform(), &config)?;
    let remote_issue = provider.fetch_issue(&remote_ref).await?;

    let status = remote_issue.status.to_ticket_status();
    let title = remote_issue.title.clone();
    let url = remote_issue.url.clone();
    let remote_ref_str = remote_ref.to_string();

    let id = create_ticket_from_remote(&remote_issue, &remote_ref, prefix)?;
    let text = format!(
        "Created {} from {}\n  Title: {}\n  URL: {}",
        id.cyan(),
        &remote_ref_str,
        &title,
        url.dimmed()
    );

    CommandOutput::new(json!({
        "id": id,
        "action": "adopted",
        "remote_ref": remote_ref_str,
        "title": title,
        "url": url,
        "status": status.to_string(),
    }))
    .with_text(&text)
    .print(output_json)
}

fn create_ticket_from_remote(
    remote_issue: &RemoteIssue,
    remote_ref: &RemoteRef,
    prefix: Option<&str>,
) -> Result<String> {
    let status = remote_issue.status.to_ticket_status();

    let priority = remote_issue.priority.unwrap_or(2);

    let body = if remote_issue.body.is_empty() {
        None
    } else {
        Some(remote_issue.body.clone())
    };

    let (id, _file_path) = TicketBuilder::new(&remote_issue.title)
        .description(body)
        .prefix(prefix)
        .ticket_type_enum(TicketType::Task)
        .status_enum(status)
        .priority(priority.to_string())
        .remote(Some(remote_ref.to_string()))
        .run_hooks(false)
        .build()?;

    Ok(id)
}

pub async fn cmd_push(local_id: &str, output_json: bool) -> Result<()> {
    let config = Config::load()?;

    let ticket = Ticket::find(local_id).await?;
    let metadata = ticket.read()?;

    if metadata.remote.is_some() {
        return Err(JanusError::AlreadyLinked(
            metadata.remote.unwrap_or_default(),
        ));
    }

    let title = metadata.title.unwrap_or_else(|| "Untitled".to_string());
    let content = ticket.read_content()?;
    let body = extract_body(&content)?;

    let default_remote = config.default_remote.as_ref().ok_or_else(|| {
        JanusError::Config(
            "No default.remote configured. Run: janus config set default.remote <platform:org>"
                .to_string(),
        )
    })?;

    let provider = create_provider(&default_remote.platform, &config)?;
    let remote_ref = provider.create_issue(&title, &body).await?;

    let remote_ref_str = remote_ref.to_string();
    ticket.update_field("remote", &remote_ref_str)?;

    let ticket_id = ticket.id.clone();
    let text = format!(
        "Created {}\nUpdated {} -> remote: {}",
        remote_ref_str.green(),
        ticket_id.cyan(),
        &remote_ref_str
    );

    CommandOutput::new(json!({
        "id": ticket_id,
        "action": "pushed",
        "remote_ref": remote_ref_str,
    }))
    .with_text(&text)
    .print(output_json)
}

pub async fn cmd_remote_link(
    local_id: &str,
    remote_ref_str: &str,
    output_json: bool,
) -> Result<()> {
    let config = Config::load()?;

    let ticket = Ticket::find(local_id).await?;
    let metadata = ticket.read()?;

    if metadata.remote.is_some() {
        return Err(JanusError::AlreadyLinked(
            metadata.remote.unwrap_or_default(),
        ));
    }

    let remote_ref = RemoteRef::parse(remote_ref_str, Some(&config))?;

    let provider = create_provider(&remote_ref.platform(), &config)?;
    let _remote_issue = provider.fetch_issue(&remote_ref).await?;

    let remote_ref_str = remote_ref.to_string();
    ticket.update_field("remote", &remote_ref_str)?;

    let ticket_id = ticket.id.clone();
    let text = format!("Linked {} -> {}", ticket_id.cyan(), remote_ref_str.green());

    CommandOutput::new(json!({
        "id": ticket_id,
        "action": "remote_linked",
        "remote_ref": remote_ref_str,
    }))
    .with_text(&text)
    .print(output_json)
}

pub async fn cmd_sync(local_id: &str, output_json: bool) -> Result<()> {
    let config = Config::load()?;

    let ticket = Ticket::find(local_id).await?;
    let metadata = ticket.read()?;

    let remote_ref_str = metadata.remote.as_ref().ok_or(JanusError::NotLinked)?;
    let remote_ref = RemoteRef::parse(remote_ref_str, Some(&config))?;

    let provider = create_provider(&remote_ref.platform(), &config)?;
    let remote_issue = provider.fetch_issue(&remote_ref).await?;

    let local_title = metadata
        .title
        .clone()
        .ok_or_else(|| JanusError::CorruptedTicket {
            id: ticket.id.clone(),
            field: "title".to_string(),
        })?;
    let local_status = metadata.status.ok_or_else(|| JanusError::CorruptedTicket {
        id: ticket.id.clone(),
        field: "status".to_string(),
    })?;
    let local_content = ticket.read_content()?;
    let _local_body = extract_body(&local_content)?;

    let sync_plan = compute_sync_state(local_title, local_status, &remote_issue);

    if output_json {
        let json_output = generate_sync_json(ticket.id.clone(), &remote_ref, &sync_plan);
        print_json(&json_output)?;
        return Ok(());
    }

    let (decisions, changes_made) = prompt_user_for_action(&sync_plan, &local_content)?;

    apply_sync_changes(&decisions, &ticket, &remote_ref, &config).await?;

    if changes_made {
        println!("\n{}", "Sync complete.".green());
    } else {
        println!("\n{}", "Already in sync.".green());
    }

    Ok(())
}
