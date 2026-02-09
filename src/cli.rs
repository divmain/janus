use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use std::io;
use std::str::FromStr;

use crate::types::{
    TicketPriority, TicketSize, TicketStatus, TicketType, DEFAULT_PRIORITY_STR, VALID_PRIORITIES,
    VALID_SIZES, VALID_STATUSES, VALID_TYPES,
};

#[derive(Parser)]
#[command(name = "janus")]
#[command(about = "Plain-text issue tracking")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
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
        #[arg(short, long, default_value = DEFAULT_PRIORITY_STR, value_parser = parse_priority)]
        priority: TicketPriority,

        /// Type: bug, feature, task, epic, chore (case-insensitive, default: task)
        #[arg(short = 't', long = "type", default_value = "task", value_parser = parse_type)]
        ticket_type: TicketType,

        /// External reference (e.g., gh-123)
        #[arg(long)]
        external_ref: Option<String>,

        /// Parent ticket ID
        #[arg(long)]
        parent: Option<String>,

        /// Custom prefix for ticket ID (e.g., 'perf' for 'perf-a982')
        #[arg(long)]
        prefix: Option<String>,

        /// ID of ticket this was spawned from (decomposition provenance)
        #[arg(long)]
        spawned_from: Option<String>,

        /// Context explaining why this ticket was spawned
        #[arg(long)]
        spawn_context: Option<String>,

        /// Size: xsmall, small, medium, large, xlarge (aliases: xs, s, m, l, xl)
        #[arg(long, value_parser = parse_size)]
        size: Option<TicketSize>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Display ticket with relationships
    #[command(visible_alias = "s")]
    Show {
        /// Ticket ID (can be partial)
        #[arg(value_parser = parse_ticket_id)]
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Open ticket in $EDITOR (requires interactive terminal unless --json is set)
    #[command(visible_alias = "e")]
    Edit {
        /// Ticket ID (can be partial)
        #[arg(value_parser = parse_ticket_id)]
        id: String,

        /// Output as JSON (prints file path without opening editor)
        #[arg(long)]
        json: bool,
    },

    /// Add timestamped note to ticket
    AddNote {
        /// Ticket ID (can be partial)
        #[arg(value_parser = parse_ticket_id)]
        id: String,

        /// Note text (provide as argument or pipe from stdin)
        #[arg(trailing_var_arg = true)]
        text: Vec<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Mark ticket as in-progress
    Start {
        /// Ticket ID (can be partial)
        #[arg(value_parser = parse_ticket_id)]
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Mark ticket as complete
    Close {
        /// Ticket ID (can be partial)
        #[arg(value_parser = parse_ticket_id)]
        id: String,

        /// Completion summary text (required unless --no-summary is used)
        #[arg(long, group = "summary_choice")]
        summary: Option<String>,

        /// Explicitly close without a summary
        #[arg(long, group = "summary_choice")]
        no_summary: bool,

        /// Mark ticket as cancelled instead of complete
        #[arg(long)]
        cancel: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Reopen a closed ticket
    Reopen {
        /// Ticket ID (can be partial)
        #[arg(value_parser = parse_ticket_id)]
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Set ticket status
    Status {
        /// Ticket ID (partial match supported)
        #[arg(value_parser = parse_ticket_id)]
        id: String,

        /// New status: new, next, in_progress, complete, cancelled (case-insensitive)
        #[arg(value_parser = parse_status)]
        status: TicketStatus,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Set a ticket field (priority, type, parent)
    Set {
        /// Ticket ID (can be partial)
        #[arg(value_parser = parse_ticket_id)]
        id: String,

        /// Field name to update (priority, type, parent)
        field: String,

        /// New value (omit to clear parent)
        value: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Manage dependencies
    Dep {
        #[command(subcommand)]
        action: DepAction,
    },

    /// Manage links
    Link {
        #[command(subcommand)]
        action: LinkAction,
    },

    /// List tickets with optional filters
    #[command(visible_alias = "l")]
    Ls {
        /// Show tickets ready to work on (no incomplete deps, status=new|next)
        #[arg(long)]
        ready: bool,

        /// Show tickets with incomplete dependencies
        #[arg(long)]
        blocked: bool,

        /// Show recently closed/cancelled tickets
        #[arg(long)]
        closed: bool,

        /// Show only active tickets (exclude closed/cancelled)
        #[arg(long, conflicts_with_all = ["ready", "blocked", "closed", "status"])]
        active: bool,

        /// Filter by specific status (mutually exclusive with --ready, --blocked, --closed, --active)
        #[arg(long, conflicts_with_all = ["ready", "blocked", "closed", "active"], value_parser = parse_status)]
        status: Option<TicketStatus>,

        /// Show tickets spawned from a specific parent (direct children only)
        #[arg(long)]
        spawned_from: Option<String>,

        /// Show tickets at specific decomposition depth (0 = root tickets)
        #[arg(long)]
        depth: Option<u32>,

        /// Show tickets up to specified depth
        #[arg(long)]
        max_depth: Option<u32>,

        /// Show next actionable tickets in a plan (uses same logic as `janus plan next`)
        #[arg(long)]
        next_in_plan: Option<String>,

        /// Filter by plan phase (cannot be used with --next-in-plan)
        #[arg(long)]
        phase: Option<u32>,

        /// Filter by triaged status (true or false)
        #[arg(long, value_parser = parse_bool_strict)]
        triaged: Option<bool>,

        /// Filter by size (can specify multiple: --size small,medium)
        #[arg(long, value_delimiter = ',', value_parser = parse_size)]
        size: Option<Vec<TicketSize>>,

        /// Maximum tickets to show (unlimited if not specified)
        #[arg(long)]
        limit: Option<usize>,

        /// Sort tickets by field (priority, created, id; default: priority)
        #[arg(long, default_value = "priority")]
        sort_by: String,

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

    /// Manage remote issues (use --help for subcommands)
    Remote {
        #[command(subcommand)]
        action: RemoteAction,
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

    /// Manage hooks
    Hook {
        #[command(subcommand)]
        action: HookAction,
    },

    /// Check ticket health - scan for corrupted or invalid ticket files
    Doctor {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Plan management
    Plan {
        #[command(subcommand)]
        action: PlanAction,
    },

    /// Output ticket relationship graphs in DOT or Mermaid format
    Graph {
        /// Show dependencies only (blocking/blocked-by relationships)
        #[arg(long)]
        deps: bool,

        /// Show spawning relationships only (parent/child via spawned-from)
        #[arg(long)]
        spawn: bool,

        /// Show both deps and spawning relationships (default, provided for explicitness)
        #[arg(long)]
        all: bool,

        /// Output format: dot (default) or mermaid
        #[arg(long, default_value = "dot")]
        format: String,

        /// Start from specific ticket (subgraph reachable from this ticket)
        #[arg(long)]
        root: Option<String>,

        /// Graph tickets in a specific plan
        #[arg(long)]
        plan: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show next ticket(s) to work on (dependency-aware)
    #[command(visible_alias = "n")]
    Next {
        /// Maximum number of tickets to show (default: 5)
        #[arg(short, long, default_value = "5")]
        limit: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for [possible values: bash, zsh, fish, powershell, elvish]
        shell: Shell,
    },

    /// Start MCP (Model Context Protocol) server for AI agent integration
    Mcp {
        /// Show MCP protocol version instead of starting server
        #[arg(long)]
        version: bool,
    },

    /// Search tickets using semantic similarity
    Search {
        /// Natural language search query (e.g., "authentication problems")
        query: String,

        /// Maximum number of results to return
        #[arg(short, long, default_value = "10")]
        limit: usize,

        /// Minimum similarity threshold (0.0-1.0, where 1.0 = identical)
        #[arg(long)]
        threshold: Option<f32>,

        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum DepAction {
    /// Add a dependency
    Add {
        /// Ticket ID
        #[arg(value_parser = parse_ticket_id)]
        id: String,
        /// Dependency ID (ticket that must be completed first)
        #[arg(value_parser = parse_ticket_id)]
        dep_id: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove a dependency
    Remove {
        /// Ticket ID
        #[arg(value_parser = parse_ticket_id)]
        id: String,
        /// Dependency ID to remove
        #[arg(value_parser = parse_ticket_id)]
        dep_id: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show dependency tree
    Tree {
        /// Ticket ID
        #[arg(value_parser = parse_ticket_id)]
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
pub enum LinkAction {
    /// Link tickets together
    Add {
        /// Ticket IDs to link
        #[arg(required = true, num_args = 2.., value_parser = parse_ticket_id)]
        ids: Vec<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove link between tickets
    Remove {
        /// First ticket ID
        #[arg(value_parser = parse_ticket_id)]
        id1: String,
        /// Second ticket ID
        #[arg(value_parser = parse_ticket_id)]
        id2: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Show current configuration
    Show {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Set a configuration value
    Set {
        /// Configuration key (github.token, linear.api_key, default.remote)
        key: String,
        /// Value to set
        value: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Get a configuration value
    Get {
        /// Configuration key (github.token, linear.api_key, default.remote)
        key: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum CacheAction {
    /// Show embedding coverage, model name, and embeddings directory size
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Delete orphaned embedding files that no longer correspond to current tickets
    Prune {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Regenerate all embeddings (deletes existing embeddings and re-embeds all tickets)
    Rebuild {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum HookAction {
    /// List configured hooks
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Install a hook recipe from GitHub
    Install {
        /// Recipe name (e.g., "git-sync")
        recipe: String,

        /// Force overwrite of existing files without prompting
        #[arg(long)]
        force: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Run a hook manually for testing
    Run {
        /// Hook event name (e.g., "post_write", "ticket_created")
        event: String,
        /// Optional item ID for context
        #[arg(long, value_parser = parse_ticket_id)]
        id: Option<String>,
    },
    /// Enable hooks
    Enable {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Disable hooks
    Disable {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// View hook failure log
    Log {
        /// Number of most recent entries to show (default: all)
        #[arg(short, long)]
        lines: Option<usize>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum RemoteAction {
    /// Browse remote issues in TUI
    Browse {
        /// Optional provider override (github or linear)
        provider: Option<String>,
    },

    /// Import a remote issue and create a local ticket
    Adopt {
        /// Remote reference (e.g., github:owner/repo/123)
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
        #[arg(value_parser = parse_ticket_id)]
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Link a local ticket to an existing remote issue
    Link {
        /// Local ticket ID (can be partial)
        #[arg(value_parser = parse_ticket_id)]
        id: String,

        /// Remote reference
        remote_ref: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Sync a local ticket with its remote issue
    Sync {
        /// Local ticket ID (can be partial)
        #[arg(value_parser = parse_ticket_id)]
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum PlanAction {
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
        #[arg(value_parser = parse_ticket_id)]
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

        /// Show full completion summaries for tickets in specified phase(s)
        #[arg(long = "verbose-phase", action = clap::ArgAction::Append)]
        verbose_phases: Vec<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Open plan in $EDITOR
    Edit {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_ticket_id)]
        id: String,

        /// Output as JSON (prints file path as JSON instead of opening editor)
        #[arg(long)]
        json: bool,
    },
    /// List all plans
    Ls {
        /// Filter by computed status
        #[arg(long, value_parser = parse_status)]
        status: Option<TicketStatus>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Add a ticket to a plan
    AddTicket {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_ticket_id)]
        plan_id: String,

        /// Ticket ID to add
        #[arg(value_parser = parse_ticket_id)]
        ticket_id: String,

        /// Target phase (required for phased plans)
        #[arg(long)]
        phase: Option<String>,

        /// Insert after specific ticket
        #[arg(long, conflicts_with = "position")]
        after: Option<String>,

        /// Insert at position (1-indexed)
        #[arg(long, conflicts_with = "after")]
        position: Option<usize>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove a ticket from a plan
    RemoveTicket {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_ticket_id)]
        plan_id: String,

        /// Ticket ID to remove
        #[arg(value_parser = parse_ticket_id)]
        ticket_id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Move a ticket between phases
    MoveTicket {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_ticket_id)]
        plan_id: String,

        /// Ticket ID to move
        #[arg(value_parser = parse_ticket_id)]
        ticket_id: String,

        /// Target phase (required)
        #[arg(long = "to-phase")]
        to_phase: String,

        /// Insert after specific ticket in target phase
        #[arg(long, conflicts_with = "position")]
        after: Option<String>,

        /// Insert at position in target phase (1-indexed)
        #[arg(long, conflicts_with = "after")]
        position: Option<usize>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Add a new phase to a plan
    AddPhase {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_ticket_id)]
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
        #[arg(value_parser = parse_ticket_id)]
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
        #[arg(value_parser = parse_ticket_id)]
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
        #[arg(value_parser = parse_ticket_id)]
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
        #[arg(value_parser = parse_ticket_id)]
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
        #[arg(value_parser = parse_ticket_id)]
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
        #[arg(value_parser = parse_ticket_id)]
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Import a plan from a markdown file
    Import {
        /// File path (use "-" for stdin)
        file: String,

        /// Validate and show what would be created without creating anything.
        /// When combined with --json, outputs a structured summary with "dry_run": true
        /// including the planned plan, tickets, and task counts.
        #[arg(long)]
        dry_run: bool,

        /// Override the extracted title
        #[arg(long)]
        title: Option<String>,

        /// Ticket type for created tasks (case-insensitive, default: task)
        #[arg(long = "type", default_value = "task", value_parser = parse_type)]
        ticket_type: TicketType,

        /// Custom prefix for created ticket IDs
        #[arg(long)]
        prefix: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show the importable plan format specification
    ImportSpec,
    /// Verify all plan files and report any errors
    Verify {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

impl Commands {
    /// Execute the command, dispatching to the appropriate handler.
    pub async fn run(self) -> crate::error::Result<()> {
        use crate::commands::{
            LsOptions, cmd_add_note, cmd_adopt, cmd_board, cmd_cache_prune, cmd_cache_rebuild,
            cmd_cache_status, cmd_close, cmd_config_get, cmd_config_set, cmd_config_show,
            cmd_create, cmd_dep_add, cmd_dep_remove, cmd_dep_tree, cmd_doctor, cmd_edit, cmd_graph,
            cmd_hook_disable, cmd_hook_enable, cmd_hook_install, cmd_hook_list, cmd_hook_log,
            cmd_hook_run, cmd_link_add, cmd_link_remove, cmd_ls_with_options, cmd_next,
            cmd_plan_add_phase, cmd_plan_add_ticket, cmd_plan_create, cmd_plan_delete,
            cmd_plan_edit, cmd_plan_import, cmd_plan_ls, cmd_plan_move_ticket, cmd_plan_next,
            cmd_plan_remove_phase, cmd_plan_remove_ticket, cmd_plan_rename, cmd_plan_reorder,
            cmd_plan_show, cmd_plan_status, cmd_plan_verify, cmd_push, cmd_query,
            cmd_remote_browse, cmd_remote_link, cmd_reopen, cmd_search, cmd_set, cmd_show,
            cmd_show_import_spec, cmd_start, cmd_status, cmd_sync, cmd_view,
        };
        use crate::error::JanusError;

        /// Handles validation results, returning Ok if valid, or an error if invalid.
        fn handle_validation_result<T>(
            result: crate::error::Result<(bool, T)>,
            error_msg: &str,
        ) -> crate::error::Result<()> {
            match result {
                Ok((valid, _)) => {
                    if valid {
                        Ok(())
                    } else {
                        Err(JanusError::InvalidInput(error_msg.to_string()))
                    }
                }
                Err(e) => Err(e),
            }
        }

        match self {
            Commands::Create {
                title,
                description,
                design,
                acceptance,
                priority,
                ticket_type,
                external_ref,
                parent,
                prefix,
                spawned_from,
                spawn_context,
                size,
                json,
            } => {
                cmd_create(
                    title,
                    description,
                    design,
                    acceptance,
                    priority,
                    ticket_type,
                    external_ref,
                    parent,
                    prefix,
                    spawned_from,
                    spawn_context,
                    size,
                    json,
                )
                .await
            }

            Commands::Show { id, json } => cmd_show(&id, json).await,
            Commands::Edit { id, json } => cmd_edit(&id, json).await,
            Commands::AddNote { id, text, json } => {
                let note_text = if text.is_empty() {
                    None
                } else {
                    Some(text.join(" "))
                };
                cmd_add_note(&id, note_text.as_deref(), json).await
            }

            Commands::Start { id, json } => cmd_start(&id, json).await,
            Commands::Close {
                id,
                summary,
                no_summary,
                cancel,
                json,
            } => cmd_close(&id, summary.as_deref(), no_summary, cancel, json).await,
            Commands::Reopen { id, json } => cmd_reopen(&id, json).await,
            Commands::Status { id, status, json } => cmd_status(&id, status, json).await,
            Commands::Set {
                id,
                field,
                value,
                json,
            } => cmd_set(&id, &field, value.as_deref(), json).await,

            Commands::Dep { action } => match action {
                DepAction::Add { id, dep_id, json } => cmd_dep_add(&id, &dep_id, json).await,
                DepAction::Remove { id, dep_id, json } => {
                    cmd_dep_remove(&id, &dep_id, json).await
                }
                DepAction::Tree { id, full, json } => cmd_dep_tree(&id, full, json).await,
            },

            Commands::Link { action } => match action {
                LinkAction::Add { ids, json } => cmd_link_add(&ids, json).await,
                LinkAction::Remove { id1, id2, json } => cmd_link_remove(&id1, &id2, json).await,
            },

            Commands::Ls {
                ready,
                blocked,
                closed,
                active,
                status,
                spawned_from,
                depth,
                max_depth,
                next_in_plan,
                phase,
                triaged,
                size,
                limit,
                sort_by,
                json,
            } => {
                let opts = LsOptions {
                    filter_ready: ready,
                    filter_blocked: blocked,
                    filter_closed: closed,
                    filter_active: active,
                    status_filter: status,
                    spawned_from,
                    depth,
                    max_depth,
                    next_in_plan,
                    phase,
                    triaged,
                    size_filter: size,
                    limit,
                    sort_by,
                    output_json: json,
                };
                cmd_ls_with_options(opts).await
            }

            Commands::Query { filter } => cmd_query(filter.as_deref()).await,

            Commands::View => cmd_view().await,
            Commands::Board => cmd_board().await,

            Commands::Remote { action } => match action {
                RemoteAction::Browse { provider } => {
                    cmd_remote_browse(provider.as_deref()).await
                }
                RemoteAction::Adopt {
                    remote_ref,
                    prefix,
                    json,
                } => cmd_adopt(&remote_ref, prefix.as_deref(), json).await,
                RemoteAction::Push { id, json } => cmd_push(&id, json).await,
                RemoteAction::Link {
                    id,
                    remote_ref,
                    json,
                } => cmd_remote_link(&id, &remote_ref, json).await,
                RemoteAction::Sync { id, json } => cmd_sync(&id, json).await,
            },

            Commands::Config { action } => match action {
                ConfigAction::Show { json } => cmd_config_show(json),
                ConfigAction::Set { key, value, json } => cmd_config_set(&key, &value, json),
                ConfigAction::Get { key, json } => cmd_config_get(&key, json),
            },

            Commands::Cache { action } => match action {
                CacheAction::Status { json } => cmd_cache_status(json).await,
                CacheAction::Prune { json } => cmd_cache_prune(json).await,
                CacheAction::Rebuild { json } => cmd_cache_rebuild(json).await,
            },

            Commands::Hook { action } => match action {
                HookAction::List { json } => cmd_hook_list(json),
                HookAction::Install {
                    recipe,
                    force,
                    json,
                } => cmd_hook_install(&recipe, force, json).await,
                HookAction::Run { event, id } => cmd_hook_run(&event, id.as_deref()).await,
                HookAction::Enable { json } => cmd_hook_enable(json),
                HookAction::Disable { json } => cmd_hook_disable(json),
                HookAction::Log { lines, json } => cmd_hook_log(lines, json),
            },

            Commands::Doctor { json } => handle_validation_result(
                cmd_doctor(json),
                "Ticket health check failed - some files have errors",
            ),

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
                    verbose_phases,
                    json,
                } => {
                    cmd_plan_show(&id, raw, tickets_only, phases_only, &verbose_phases, json)
                        .await
                }
                PlanAction::Edit { id, json } => cmd_plan_edit(&id, json).await,
                PlanAction::Ls { status, json } => cmd_plan_ls(status, json).await,
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
                } => {
                    cmd_plan_add_phase(&plan_id, &phase_name, after.as_deref(), position, json)
                        .await
                }
                PlanAction::RemovePhase {
                    plan_id,
                    phase,
                    force,
                    migrate,
                    json,
                } => {
                    cmd_plan_remove_phase(&plan_id, &phase, force, migrate.as_deref(), json).await
                }
                PlanAction::Reorder {
                    plan_id,
                    phase,
                    reorder_phases,
                    json,
                } => cmd_plan_reorder(&plan_id, phase.as_deref(), reorder_phases, json).await,
                PlanAction::Delete { id, force, json } => {
                    cmd_plan_delete(&id, force, json).await
                }
                PlanAction::Rename {
                    id,
                    new_title,
                    json,
                } => cmd_plan_rename(&id, &new_title, json).await,
                PlanAction::Next {
                    id,
                    phase,
                    all,
                    count,
                    json,
                } => cmd_plan_next(&id, phase, all, count, json).await,
                PlanAction::Status { id, json } => cmd_plan_status(&id, json).await,
                PlanAction::Import {
                    file,
                    dry_run,
                    title,
                    ticket_type,
                    prefix,
                    json,
                } => {
                    cmd_plan_import(
                        &file,
                        dry_run,
                        title.as_deref(),
                        ticket_type,
                        prefix.as_deref(),
                        json,
                    )
                    .await
                }
                PlanAction::ImportSpec => cmd_show_import_spec(),
                PlanAction::Verify { json } => handle_validation_result(
                    cmd_plan_verify(json),
                    "Plan verification failed - some files have errors",
                ),
            },

            Commands::Graph {
                deps,
                spawn,
                all,
                format,
                root,
                plan,
                json,
            } => {
                cmd_graph(
                    deps,
                    spawn,
                    all,
                    &format,
                    root.as_deref(),
                    plan.as_deref(),
                    json,
                )
                .await
            }

            Commands::Next { limit, json } => cmd_next(limit, json).await,

            Commands::Completions { shell } => {
                generate_completions(shell);
                Ok(())
            }

            Commands::Mcp { version } => {
                if version {
                    crate::mcp::cmd_mcp_version()
                } else {
                    crate::mcp::cmd_mcp().await
                }
            }

            Commands::Search {
                query,
                limit,
                threshold,
                json,
            } => cmd_search(&query, limit, threshold, json).await,
        }
    }
}

/// Generic validation helper for parsing values with a standard error message format.
fn parse_with_validation<T, F>(
    s: &str,
    parser: F,
    field_name: &str,
    valid_values: &[&str],
) -> Result<T, String>
where
    F: FnOnce(&str) -> Result<T, String>,
{
    parser(s).map_err(|_| {
        format!(
            "Invalid {}. Must be one of: {}",
            field_name,
            valid_values.join(", ")
        )
    })
}

fn parse_priority(s: &str) -> Result<TicketPriority, String> {
    parse_with_validation(
        s,
        |v| v.parse().map_err(|_| String::new()),
        "priority",
        VALID_PRIORITIES,
    )
}

fn parse_type(s: &str) -> Result<TicketType, String> {
    parse_with_validation(
        s,
        |v| v.parse().map_err(|_| String::new()),
        "type",
        VALID_TYPES,
    )
}

fn parse_status(s: &str) -> Result<TicketStatus, String> {
    parse_with_validation(
        s,
        |v| TicketStatus::from_str(v).map_err(|_| String::new()),
        "status",
        VALID_STATUSES,
    )
}

fn parse_ticket_id(s: &str) -> Result<String, String> {
    if s.is_empty() {
        return Err("ID cannot be empty".to_string());
    }

    if s.chars().all(char::is_whitespace) {
        return Err("ID cannot be only whitespace".to_string());
    }

    if !s
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "ID must contain only alphanumeric characters, hyphens, and underscores".to_string(),
        );
    }

    Ok(s.to_string())
}

fn parse_bool_strict(s: &str) -> Result<bool, String> {
    match s.to_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!(
            "Invalid boolean value '{s}'. Must be 'true' or 'false'"
        )),
    }
}

fn parse_size(s: &str) -> Result<TicketSize, String> {
    let mut valid_values = VALID_SIZES.to_vec();
    valid_values.extend(["xs", "s", "m", "l", "xl"]);
    parse_with_validation(
        s,
        |v| v.parse().map_err(|_| String::new()),
        "size",
        &valid_values,
    )
}

pub fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "janus", &mut io::stdout());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bool_strict_accepts_true() {
        assert_eq!(parse_bool_strict("true").unwrap(), true);
        assert_eq!(parse_bool_strict("True").unwrap(), true);
        assert_eq!(parse_bool_strict("TRUE").unwrap(), true);
    }

    #[test]
    fn test_parse_bool_strict_accepts_false() {
        assert_eq!(parse_bool_strict("false").unwrap(), false);
        assert_eq!(parse_bool_strict("False").unwrap(), false);
        assert_eq!(parse_bool_strict("FALSE").unwrap(), false);
    }

    #[test]
    fn test_parse_bool_strict_rejects_invalid() {
        assert!(parse_bool_strict("yes").is_err());
        assert!(parse_bool_strict("no").is_err());
        assert!(parse_bool_strict("1").is_err());
        assert!(parse_bool_strict("0").is_err());
        assert!(parse_bool_strict("").is_err());
        assert!(parse_bool_strict("tru").is_err());
        assert!(parse_bool_strict("fals").is_err());
    }

    #[test]
    fn test_parse_bool_strict_error_message() {
        let err = parse_bool_strict("yes").unwrap_err();
        assert!(
            err.contains("yes"),
            "Error should contain the invalid value"
        );
        assert!(
            err.contains("true") && err.contains("false"),
            "Error should list valid values"
        );
    }

    #[test]
    fn test_parse_status_valid() {
        assert_eq!(parse_status("new").unwrap(), TicketStatus::New);
        assert_eq!(parse_status("next").unwrap(), TicketStatus::Next);
        assert_eq!(
            parse_status("in_progress").unwrap(),
            TicketStatus::InProgress
        );
        assert_eq!(parse_status("complete").unwrap(), TicketStatus::Complete);
        assert_eq!(parse_status("cancelled").unwrap(), TicketStatus::Cancelled);
    }

    #[test]
    fn test_parse_status_case_insensitive() {
        assert_eq!(parse_status("NEW").unwrap(), TicketStatus::New);
        assert_eq!(
            parse_status("IN_PROGRESS").unwrap(),
            TicketStatus::InProgress
        );
    }

    #[test]
    fn test_parse_status_invalid_rejected() {
        assert!(parse_status("typo").is_err());
        assert!(parse_status("open").is_err());
        assert!(parse_status("done").is_err());
        assert!(parse_status("").is_err());
    }

    #[test]
    fn test_parse_status_error_message_lists_valid_values() {
        let err = parse_status("typo").unwrap_err();
        assert!(
            err.contains("new") && err.contains("in_progress") && err.contains("complete"),
            "Error should list valid status values, got: {err}"
        );
    }
}
