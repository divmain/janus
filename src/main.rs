use clap::{Parser, Subcommand};
use std::process::ExitCode;

use janus::commands::{
    CreateOptions, cmd_add_note, cmd_adopt, cmd_blocked, cmd_board, cmd_cache_clear,
    cmd_cache_path, cmd_cache_rebuild, cmd_cache_status, cmd_close, cmd_closed, cmd_config_get,
    cmd_config_set, cmd_config_show, cmd_create, cmd_dep_add, cmd_dep_remove, cmd_dep_tree,
    cmd_edit, cmd_link_add, cmd_link_remove, cmd_ls, cmd_plan_add_phase, cmd_plan_add_ticket,
    cmd_plan_create, cmd_plan_delete, cmd_plan_edit, cmd_plan_ls, cmd_plan_move_ticket,
    cmd_plan_next, cmd_plan_remove_phase, cmd_plan_remove_ticket, cmd_plan_rename,
    cmd_plan_reorder, cmd_plan_show, cmd_plan_status, cmd_push, cmd_query, cmd_ready,
    cmd_remote_link, cmd_remote_tui, cmd_reopen, cmd_show, cmd_start, cmd_status, cmd_sync,
    cmd_view,
};
use janus::types::{TicketPriority, TicketType, VALID_PRIORITIES, VALID_STATUSES, VALID_TYPES};

#[derive(Parser)]
#[command(name = "janus")]
#[command(about = "Plain-text issue tracking")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new ticket
    #[command(visible_alias = "c")]
    Create {
        /// Ticket title
        title: String,

        /// Description text
        #[arg(short, long)]
        description: Option<String>,

        /// Design notes
        #[arg(long)]
        design: Option<String>,

        /// Acceptance criteria
        #[arg(long)]
        acceptance: Option<String>,

        /// Priority (0-4, default: 2)
        #[arg(short, long, default_value = "2", value_parser = parse_priority)]
        priority: TicketPriority,

        /// Type: bug, feature, task, epic, chore (default: task)
        #[arg(short = 't', long = "type", default_value = "task", value_parser = parse_type)]
        ticket_type: TicketType,

        /// Assignee (default: git user.name)
        #[arg(short, long)]
        assignee: Option<String>,

        /// External reference (e.g., gh-123)
        #[arg(long)]
        external_ref: Option<String>,

        /// Parent ticket ID
        #[arg(long)]
        parent: Option<String>,

        /// Custom prefix for ticket ID (e.g., 'perf' for 'perf-a982')
        #[arg(long)]
        prefix: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Display ticket with relationships
    #[command(visible_alias = "s")]
    Show {
        /// Ticket ID (can be partial)
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Open ticket in $EDITOR
    Edit {
        /// Ticket ID (can be partial)
        id: String,

        /// Output as JSON (prints file path as JSON instead of opening editor)
        #[arg(long)]
        json: bool,
    },

    /// Add timestamped note to ticket
    AddNote {
        /// Ticket ID (can be partial)
        id: String,

        /// Note text (reads from stdin if not provided)
        #[arg(trailing_var_arg = true)]
        text: Vec<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Mark ticket as in-progress
    Start {
        /// Ticket ID (can be partial)
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Mark ticket as complete
    Close {
        /// Ticket ID (can be partial)
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Reopen a closed ticket
    Reopen {
        /// Ticket ID (can be partial)
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Set ticket status
    Status {
        /// Ticket ID (can be partial)
        id: String,

        /// New status (new, next, in_progress, complete, cancelled)
        #[arg(value_parser = parse_status)]
        status: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Manage dependencies
    Dep {
        #[command(subcommand)]
        action: DepAction,
    },

    /// Legacy: add dependency (hidden, use `dep add` instead)
    #[command(hide = true)]
    Undep {
        /// Ticket ID
        id: String,
        /// Dependency ID to remove
        dep_id: String,
    },

    /// Manage links
    Link {
        #[command(subcommand)]
        action: LinkAction,
    },

    /// Legacy: remove link (hidden, use `link remove` instead)
    #[command(hide = true)]
    Unlink {
        /// First ticket ID
        id1: String,
        /// Second ticket ID
        id2: String,
    },

    /// List all tickets
    Ls {
        /// Filter by status
        #[arg(long)]
        status: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// List tickets ready to work on
    Ready {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// List blocked tickets
    Blocked {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// List recently closed tickets
    Closed {
        /// Maximum number of tickets to show
        #[arg(long, default_value = "20")]
        limit: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Output tickets as JSON, optionally filtered with jq syntax
    Query {
        /// jq filter expression (e.g., '.status == "new"')
        filter: Option<String>,
    },

    /// Browse issues with fuzzy search
    View,

    /// View issues on a Kanban board
    Board,

    // Remote sync commands
    /// Adopt a remote issue and create a local ticket
    Adopt {
        /// Remote reference (e.g., github:owner/repo/123 or linear:org/PROJ-123)
        remote_ref: String,

        /// Custom prefix for ticket ID (e.g., 'perf' for 'perf-a982')
        #[arg(long)]
        prefix: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Push a local ticket to create a remote issue
    Push {
        /// Local ticket ID (can be partial)
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Link a local ticket to an existing remote issue
    RemoteLink {
        /// Local ticket ID (can be partial)
        id: String,
        /// Remote reference (e.g., github:owner/repo/123 or linear:org/PROJ-123)
        remote_ref: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Sync a local ticket with its remote issue
    Sync {
        /// Local ticket ID (can be partial)
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// TUI for managing remote issues
    Remote {
        /// Optional provider override (github or linear)
        #[arg(value_name = "provider")]
        provider: Option<String>,
    },

    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Cache management
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },

    /// Plan management
    Plan {
        #[command(subcommand)]
        action: PlanAction,
    },
}

#[derive(Subcommand)]
enum DepAction {
    /// Add a dependency
    Add {
        /// Ticket ID
        id: String,
        /// Dependency ID (ticket that must be completed first)
        dep_id: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove a dependency
    Remove {
        /// Ticket ID
        id: String,
        /// Dependency ID to remove
        dep_id: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show dependency tree
    Tree {
        /// Ticket ID
        id: String,
        /// Show full tree (including duplicate nodes)
        #[arg(long)]
        full: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum LinkAction {
    /// Link tickets together
    Add {
        /// Ticket IDs to link
        #[arg(required = true, num_args = 2..)]
        ids: Vec<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove link between tickets
    Remove {
        /// First ticket ID
        id1: String,
        /// Second ticket ID
        id2: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show current configuration
    Show {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Set a configuration value
    Set {
        /// Configuration key (github.token, linear.api_key, default_remote)
        key: String,
        /// Value to set
        value: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Get a configuration value
    Get {
        /// Configuration key (github.token, linear.api_key, default_remote)
        key: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum CacheAction {
    /// Show cache status
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Clear cache for current repo
    Clear {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Force full cache rebuild
    Rebuild {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Print path to cache database
    Path {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum PlanAction {
    /// Create a new plan
    Create {
        /// Plan title
        title: String,

        /// Add initial phase (creates a phased plan), can be repeated
        #[arg(long = "phase", action = clap::ArgAction::Append)]
        phases: Vec<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Display a plan with full details
    Show {
        /// Plan ID (can be partial)
        id: String,

        /// Show raw file content instead of enhanced output
        #[arg(long)]
        raw: bool,

        /// Show only the ticket list with statuses
        #[arg(long = "tickets-only")]
        tickets_only: bool,

        /// Show only phase summary (phased plans)
        #[arg(long = "phases-only")]
        phases_only: bool,

        /// Output format (text, json)
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Open plan in $EDITOR
    Edit {
        /// Plan ID (can be partial)
        id: String,

        /// Output as JSON (prints file path as JSON instead of opening editor)
        #[arg(long)]
        json: bool,
    },
    /// List all plans
    Ls {
        /// Filter by computed status
        #[arg(long)]
        status: Option<String>,

        /// Output format (text, json)
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Add a ticket to a plan
    AddTicket {
        /// Plan ID (can be partial)
        plan_id: String,

        /// Ticket ID to add
        ticket_id: String,

        /// Target phase (required for phased plans)
        #[arg(long)]
        phase: Option<String>,

        /// Insert after specific ticket
        #[arg(long)]
        after: Option<String>,

        /// Insert at position (1-indexed)
        #[arg(long)]
        position: Option<usize>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove a ticket from a plan
    RemoveTicket {
        /// Plan ID (can be partial)
        plan_id: String,

        /// Ticket ID to remove
        ticket_id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Move a ticket between phases
    MoveTicket {
        /// Plan ID (can be partial)
        plan_id: String,

        /// Ticket ID to move
        ticket_id: String,

        /// Target phase (required)
        #[arg(long = "to-phase")]
        to_phase: String,

        /// Insert after specific ticket in target phase
        #[arg(long)]
        after: Option<String>,

        /// Insert at position in target phase (1-indexed)
        #[arg(long)]
        position: Option<usize>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Add a new phase to a plan
    AddPhase {
        /// Plan ID (can be partial)
        plan_id: String,

        /// Phase name
        phase_name: String,

        /// Insert after specific phase
        #[arg(long)]
        after: Option<String>,

        /// Insert at position (1-indexed)
        #[arg(long)]
        position: Option<usize>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove a phase from a plan
    RemovePhase {
        /// Plan ID (can be partial)
        plan_id: String,

        /// Phase name or number
        phase: String,

        /// Force removal even if phase contains tickets
        #[arg(long)]
        force: bool,

        /// Move tickets to another phase instead of removing
        #[arg(long)]
        migrate: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Reorder tickets or phases
    Reorder {
        /// Plan ID (can be partial)
        plan_id: String,

        /// Reorder tickets within a specific phase
        #[arg(long)]
        phase: Option<String>,

        /// Reorder the phases themselves (not tickets within a phase)
        #[arg(long = "reorder-phases")]
        reorder_phases: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Delete a plan
    Delete {
        /// Plan ID (can be partial)
        id: String,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Rename a plan (update its title)
    Rename {
        /// Plan ID (can be partial)
        id: String,

        /// New title
        new_title: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show the next actionable item(s) in a plan
    Next {
        /// Plan ID (can be partial)
        id: String,

        /// Show next item in current phase only
        #[arg(long)]
        phase: bool,

        /// Show next item for each incomplete phase
        #[arg(long)]
        all: bool,

        /// Number of next items to show (default: 1)
        #[arg(long, default_value = "1")]
        count: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show plan status summary
    Status {
        /// Plan ID (can be partial)
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

fn parse_priority(s: &str) -> Result<TicketPriority, String> {
    s.parse().map_err(|_| {
        format!(
            "Invalid priority. Must be one of: {}",
            VALID_PRIORITIES.join(", ")
        )
    })
}

fn parse_type(s: &str) -> Result<TicketType, String> {
    s.parse()
        .map_err(|_| format!("Invalid type. Must be one of: {}", VALID_TYPES.join(", ")))
}

fn parse_status(s: &str) -> Result<String, String> {
    if VALID_STATUSES.contains(&s) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "Invalid status. Must be one of: {}",
            VALID_STATUSES.join(", ")
        ))
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Create {
            title,
            description,
            design,
            acceptance,
            priority,
            ticket_type,
            assignee,
            external_ref,
            parent,
            prefix,
            json,
        } => cmd_create(
            CreateOptions {
                title,
                description,
                design,
                acceptance,
                priority,
                ticket_type,
                assignee,
                external_ref,
                parent,
                prefix,
            },
            json,
        ),

        Commands::Show { id, json } => cmd_show(&id, json).await,
        Commands::Edit { id, json } => cmd_edit(&id, json),
        Commands::AddNote { id, text, json } => {
            let note_text = if text.is_empty() {
                None
            } else {
                Some(text.join(" "))
            };
            cmd_add_note(&id, note_text.as_deref(), json)
        }

        Commands::Start { id, json } => cmd_start(&id, json),
        Commands::Close { id, json } => cmd_close(&id, json),
        Commands::Reopen { id, json } => cmd_reopen(&id, json),
        Commands::Status { id, status, json } => cmd_status(&id, &status, json),

        Commands::Dep { action } => match action {
            DepAction::Add { id, dep_id, json } => cmd_dep_add(&id, &dep_id, json),
            DepAction::Remove { id, dep_id, json } => cmd_dep_remove(&id, &dep_id, json),
            DepAction::Tree { id, full, json } => cmd_dep_tree(&id, full, json).await,
        },
        Commands::Undep { id, dep_id } => cmd_dep_remove(&id, &dep_id, false),

        Commands::Link { action } => match action {
            LinkAction::Add { ids, json } => cmd_link_add(&ids, json),
            LinkAction::Remove { id1, id2, json } => cmd_link_remove(&id1, &id2, json),
        },
        Commands::Unlink { id1, id2 } => cmd_link_remove(&id1, &id2, false),

        Commands::Ls { status, json } => cmd_ls(status.as_deref(), json).await,
        Commands::Ready { json } => cmd_ready(json).await,
        Commands::Blocked { json } => cmd_blocked(json).await,
        Commands::Closed { limit, json } => cmd_closed(limit, json),

        Commands::Query { filter } => cmd_query(filter.as_deref()).await,

        // TUI commands
        Commands::View => cmd_view(),
        Commands::Board => cmd_board(),

        // Remote sync commands
        Commands::Adopt {
            remote_ref,
            prefix,
            json,
        } => cmd_adopt(&remote_ref, prefix.as_deref(), json).await,
        Commands::Push { id, json } => cmd_push(&id, json).await,
        Commands::RemoteLink {
            id,
            remote_ref,
            json,
        } => cmd_remote_link(&id, &remote_ref, json).await,
        Commands::Sync { id, json } => cmd_sync(&id, json).await,
        Commands::Remote { provider } => cmd_remote_tui(provider.as_deref()),

        // Configuration commands
        Commands::Config { action } => match action {
            ConfigAction::Show { json } => cmd_config_show(json),
            ConfigAction::Set { key, value, json } => cmd_config_set(&key, &value, json),
            ConfigAction::Get { key, json } => cmd_config_get(&key, json),
        },

        // Cache commands
        Commands::Cache { action } => match action {
            CacheAction::Status { json } => cmd_cache_status(json).await,
            CacheAction::Clear { json } => cmd_cache_clear(json).await,
            CacheAction::Rebuild { json } => cmd_cache_rebuild(json).await,
            CacheAction::Path { json } => cmd_cache_path(json).await,
        },

        // Plan commands
        Commands::Plan { action } => match action {
            PlanAction::Create {
                title,
                phases,
                json,
            } => cmd_plan_create(&title, &phases, json),
            PlanAction::Show {
                id,
                raw,
                tickets_only,
                phases_only,
                format,
            } => cmd_plan_show(&id, raw, tickets_only, phases_only, &format).await,
            PlanAction::Edit { id, json } => cmd_plan_edit(&id, json),
            PlanAction::Ls { status, format } => cmd_plan_ls(status.as_deref(), &format).await,
            PlanAction::AddTicket {
                plan_id,
                ticket_id,
                phase,
                after,
                position,
                json,
            } => {
                cmd_plan_add_ticket(
                    &plan_id,
                    &ticket_id,
                    phase.as_deref(),
                    after.as_deref(),
                    position,
                    json,
                )
                .await
            }
            PlanAction::RemoveTicket {
                plan_id,
                ticket_id,
                json,
            } => cmd_plan_remove_ticket(&plan_id, &ticket_id, json).await,
            PlanAction::MoveTicket {
                plan_id,
                ticket_id,
                to_phase,
                after,
                position,
                json,
            } => {
                cmd_plan_move_ticket(
                    &plan_id,
                    &ticket_id,
                    &to_phase,
                    after.as_deref(),
                    position,
                    json,
                )
                .await
            }
            PlanAction::AddPhase {
                plan_id,
                phase_name,
                after,
                position,
                json,
            } => cmd_plan_add_phase(&plan_id, &phase_name, after.as_deref(), position, json),
            PlanAction::RemovePhase {
                plan_id,
                phase,
                force,
                migrate,
                json,
            } => cmd_plan_remove_phase(&plan_id, &phase, force, migrate.as_deref(), json),
            PlanAction::Reorder {
                plan_id,
                phase,
                reorder_phases,
                json,
            } => cmd_plan_reorder(&plan_id, phase.as_deref(), reorder_phases, json),
            PlanAction::Delete { id, force, json } => cmd_plan_delete(&id, force, json),
            PlanAction::Rename {
                id,
                new_title,
                json,
            } => cmd_plan_rename(&id, &new_title, json),
            PlanAction::Next {
                id,
                phase,
                all,
                count,
                json,
            } => cmd_plan_next(&id, phase, all, count, json).await,
            PlanAction::Status { id, json } => cmd_plan_status(&id, json).await,
        },
    };

    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}", e);
            ExitCode::FAILURE
        }
    }
}
