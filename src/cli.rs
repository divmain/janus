use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use std::io;
use std::str::FromStr;

use crate::query::SortField;
use crate::types::{DEFAULT_PRIORITY_STR, TicketPriority, TicketSize, TicketStatus, TicketType};

/// Shared output options for commands that support JSON output.
#[derive(Args, Clone, Copy, Debug)]
pub struct OutputOptions {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

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

        #[command(flatten)]
        output: OutputOptions,
    },

    /// Display ticket with relationships
    #[command(visible_alias = "s")]
    Show {
        /// Ticket ID (can be partial)
        #[arg(value_parser = parse_partial_id)]
        id: String,

        #[command(flatten)]
        output: OutputOptions,
    },

    /// Open ticket in $EDITOR (requires interactive terminal unless --json is set)
    #[command(visible_alias = "e")]
    Edit {
        /// Ticket ID (can be partial)
        #[arg(value_parser = parse_partial_id)]
        id: String,

        #[command(flatten)]
        output: OutputOptions,
    },

    /// Add timestamped note to ticket
    AddNote {
        /// Ticket ID (can be partial)
        #[arg(value_parser = parse_partial_id)]
        id: String,

        /// Note text (provide as argument or pipe from stdin)
        #[arg(trailing_var_arg = true)]
        text: Vec<String>,

        #[command(flatten)]
        output: OutputOptions,
    },

    /// Mark ticket as in-progress
    Start {
        /// Ticket ID (can be partial)
        #[arg(value_parser = parse_partial_id)]
        id: String,

        #[command(flatten)]
        output: OutputOptions,
    },

    /// Mark ticket as complete or cancelled (enforces completion summary).
    ///
    /// Requires either --summary or --no-summary to ensure a conscious decision
    /// about completion documentation. For scripting or automation where summary
    /// enforcement isn't needed, use `janus status <id> complete` instead.
    Close {
        /// Ticket ID (can be partial)
        #[arg(value_parser = parse_partial_id)]
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

        #[command(flatten)]
        output: OutputOptions,
    },

    /// Reopen a closed ticket
    Reopen {
        /// Ticket ID (can be partial)
        #[arg(value_parser = parse_partial_id)]
        id: String,

        #[command(flatten)]
        output: OutputOptions,
    },

    /// Set ticket status (low-level field setter).
    ///
    /// Directly sets the status field without enforcing completion summaries.
    /// For completing tickets with documentation, prefer `janus close` which
    /// requires a summary or explicit opt-out.
    Status {
        /// Ticket ID (partial match supported)
        #[arg(value_parser = parse_partial_id)]
        id: String,

        /// New status: new, next, in_progress, complete, cancelled (case-insensitive)
        #[arg(value_parser = parse_status)]
        status: TicketStatus,

        #[command(flatten)]
        output: OutputOptions,
    },

    /// Set a ticket field (priority, type, parent)
    Set {
        /// Ticket ID (can be partial)
        #[arg(value_parser = parse_partial_id)]
        id: String,

        /// Field name to update (priority, type, parent)
        field: String,

        /// New value (omit to clear parent)
        value: Option<String>,

        #[command(flatten)]
        output: OutputOptions,
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
        #[arg(long, default_value = "priority", value_parser = parse_sort_field)]
        sort_by: SortField,

        #[command(flatten)]
        output: OutputOptions,
    },

    /// Output tickets as JSON, optionally filtered with jq syntax
    Query {
        /// Boolean expression for jq's select() function. The expression is wrapped
        /// in select(...) before being passed to jq. Requires jq to be installed.
        /// Example: '.status == "new"' becomes select(.status == "new")
        #[arg(long)]
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

    /// Event log management
    Events {
        #[command(subcommand)]
        action: EventsAction,
    },

    /// Manage hooks
    Hook {
        #[command(subcommand)]
        action: HookAction,
    },

    /// Check ticket health - scan for corrupted or invalid ticket files
    Doctor {
        #[command(flatten)]
        output: OutputOptions,
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

        #[command(flatten)]
        output: OutputOptions,
    },

    /// Show next ticket(s) to work on (dependency-aware)
    #[command(visible_alias = "n")]
    Next {
        /// Maximum number of tickets to show (default: 5)
        #[arg(short, long, default_value = "5")]
        limit: usize,

        #[command(flatten)]
        output: OutputOptions,
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

        #[command(flatten)]
        output: OutputOptions,
    },
}

#[derive(Subcommand)]
pub enum DepAction {
    /// Add a dependency
    Add {
        /// Ticket ID
        #[arg(value_parser = parse_partial_id)]
        id: String,
        /// Dependency ID (ticket that must be completed first)
        #[arg(value_parser = parse_partial_id)]
        dep_id: String,

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Remove a dependency
    Remove {
        /// Ticket ID
        #[arg(value_parser = parse_partial_id)]
        id: String,
        /// Dependency ID to remove
        #[arg(value_parser = parse_partial_id)]
        dep_id: String,

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Show dependency tree
    Tree {
        /// Ticket ID
        #[arg(value_parser = parse_partial_id)]
        id: String,
        /// Show full tree (including duplicate nodes)
        #[arg(long)]
        full: bool,

        #[command(flatten)]
        output: OutputOptions,
    },
}

#[derive(Subcommand)]
pub enum LinkAction {
    /// Link tickets together
    Add {
        /// Ticket IDs to link
        #[arg(required = true, num_args = 2.., value_parser = parse_partial_id)]
        ids: Vec<String>,

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Remove link between tickets
    Remove {
        /// First ticket ID
        #[arg(value_parser = parse_partial_id)]
        id1: String,
        /// Second ticket ID
        #[arg(value_parser = parse_partial_id)]
        id2: String,

        #[command(flatten)]
        output: OutputOptions,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Show current configuration
    Show {
        #[command(flatten)]
        output: OutputOptions,
    },
    /// Set a configuration value
    Set {
        /// Configuration key (github.token, linear.api_key, default.remote)
        key: String,
        /// Value to set
        value: String,

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Get a configuration value
    Get {
        /// Configuration key (github.token, linear.api_key, default.remote)
        key: String,

        #[command(flatten)]
        output: OutputOptions,
    },
}

#[derive(Subcommand)]
pub enum CacheAction {
    /// Show embedding coverage, model name, and embeddings directory size
    Status {
        #[command(flatten)]
        output: OutputOptions,
    },
    /// Delete orphaned embedding files that no longer correspond to current tickets
    Prune {
        #[command(flatten)]
        output: OutputOptions,
    },
    /// Regenerate all embeddings (deletes existing embeddings and re-embeds all tickets)
    Rebuild {
        #[command(flatten)]
        output: OutputOptions,
    },
}

#[derive(Subcommand)]
pub enum EventsAction {
    /// Clear the events log file
    Prune {
        #[command(flatten)]
        output: OutputOptions,
    },
}

#[derive(Subcommand)]
pub enum HookAction {
    /// List configured hooks
    List {
        #[command(flatten)]
        output: OutputOptions,
    },
    /// Install a hook recipe from GitHub
    Install {
        /// Recipe name (e.g., "git-sync")
        recipe: String,

        /// Force overwrite of existing files without prompting
        #[arg(long)]
        force: bool,

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Run a hook manually for testing
    Run {
        /// Hook event name (e.g., "post_write", "ticket_created")
        event: String,
        /// Optional item ID for context
        #[arg(long, value_parser = parse_partial_id)]
        id: Option<String>,
    },
    /// Enable hooks
    Enable {
        #[command(flatten)]
        output: OutputOptions,
    },
    /// Disable hooks
    Disable {
        #[command(flatten)]
        output: OutputOptions,
    },
    /// View hook failure log
    Log {
        /// Number of most recent entries to show (default: all)
        #[arg(short, long)]
        lines: Option<usize>,

        #[command(flatten)]
        output: OutputOptions,
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

        #[command(flatten)]
        output: OutputOptions,
    },

    /// Push a local ticket to create a remote issue
    Push {
        /// Local ticket ID (can be partial)
        #[arg(value_parser = parse_partial_id)]
        id: String,

        #[command(flatten)]
        output: OutputOptions,
    },

    /// Link a local ticket to an existing remote issue
    Link {
        /// Local ticket ID (can be partial)
        #[arg(value_parser = parse_partial_id)]
        id: String,

        /// Remote reference
        remote_ref: String,

        #[command(flatten)]
        output: OutputOptions,
    },

    /// Sync a local ticket with its remote issue
    Sync {
        /// Local ticket ID (can be partial)
        #[arg(value_parser = parse_partial_id)]
        id: String,

        #[command(flatten)]
        output: OutputOptions,
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

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Display a plan with full details
    Show {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_plan_id)]
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

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Open plan in $EDITOR
    Edit {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_plan_id)]
        id: String,

        #[command(flatten)]
        output: OutputOptions,
    },
    /// List all plans
    Ls {
        /// Filter by computed status
        #[arg(long, value_parser = parse_status)]
        status: Option<TicketStatus>,

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Add a ticket to a plan
    AddTicket {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_plan_id)]
        plan_id: String,

        /// Ticket ID to add
        #[arg(value_parser = parse_partial_id)]
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

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Remove a ticket from a plan
    RemoveTicket {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_plan_id)]
        plan_id: String,

        /// Ticket ID to remove
        #[arg(value_parser = parse_partial_id)]
        ticket_id: String,

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Move a ticket between phases
    MoveTicket {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_plan_id)]
        plan_id: String,

        /// Ticket ID to move
        #[arg(value_parser = parse_partial_id)]
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

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Add a new phase to a plan
    AddPhase {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_plan_id)]
        plan_id: String,

        /// Phase name
        phase_name: String,

        /// Insert after specific phase
        #[arg(long)]
        after: Option<String>,

        /// Insert at position (1-indexed)
        #[arg(long)]
        position: Option<usize>,

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Remove a phase from a plan
    RemovePhase {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_plan_id)]
        plan_id: String,

        /// Phase name or number
        phase: String,

        /// Force removal even if phase contains tickets
        #[arg(long)]
        force: bool,

        /// Move tickets to another phase instead of removing
        #[arg(long)]
        migrate: Option<String>,

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Reorder tickets or phases
    Reorder {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_plan_id)]
        plan_id: String,

        /// Reorder tickets within a specific phase
        #[arg(long)]
        phase: Option<String>,

        /// Reorder the phases themselves (not tickets within a phase)
        #[arg(long = "reorder-phases")]
        reorder_phases: bool,

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Delete a plan
    Delete {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_plan_id)]
        id: String,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Rename a plan (update its title)
    Rename {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_plan_id)]
        id: String,

        /// New title
        new_title: String,

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Show the next actionable item(s) in a plan
    Next {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_plan_id)]
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

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Show plan status summary
    Status {
        /// Plan ID (can be partial)
        #[arg(value_parser = parse_plan_id)]
        id: String,

        #[command(flatten)]
        output: OutputOptions,
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

        #[command(flatten)]
        output: OutputOptions,
    },
    /// Show the importable plan format specification
    ImportSpec,
    /// Verify all plan files and report any errors
    Verify {
        #[command(flatten)]
        output: OutputOptions,
    },
}

impl Commands {
    /// Execute the command, dispatching to the appropriate handler.
    pub async fn run(self) -> crate::error::Result<()> {
        use crate::commands::{
            CreateOptions, LsOptions, cmd_add_note, cmd_adopt, cmd_board, cmd_cache_prune,
            cmd_cache_rebuild, cmd_cache_status, cmd_close, cmd_config_get, cmd_config_set,
            cmd_config_show, cmd_create, cmd_dep_add, cmd_dep_remove, cmd_dep_tree, cmd_doctor,
            cmd_edit, cmd_events_prune, cmd_graph, cmd_hook_disable, cmd_hook_enable,
            cmd_hook_install, cmd_hook_list, cmd_hook_log, cmd_hook_run, cmd_link_add,
            cmd_link_remove, cmd_ls_with_options, cmd_next, cmd_plan_add_phase,
            cmd_plan_add_ticket, cmd_plan_create, cmd_plan_delete, cmd_plan_edit, cmd_plan_import,
            cmd_plan_ls, cmd_plan_move_ticket, cmd_plan_next, cmd_plan_remove_phase,
            cmd_plan_remove_ticket, cmd_plan_rename, cmd_plan_reorder, cmd_plan_show,
            cmd_plan_status, cmd_plan_verify, cmd_push, cmd_query, cmd_remote_browse,
            cmd_remote_link, cmd_reopen, cmd_search, cmd_set, cmd_show, cmd_show_import_spec,
            cmd_start, cmd_status, cmd_sync, cmd_view,
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
                output,
            } => {
                cmd_create(CreateOptions {
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
                    output,
                })
                .await
            }

            Commands::Show { id, output } => cmd_show(&id, output).await,
            Commands::Edit { id, output } => cmd_edit(&id, output).await,
            Commands::AddNote { id, text, output } => {
                let note_text = if text.is_empty() {
                    None
                } else {
                    Some(text.join(" "))
                };
                cmd_add_note(&id, note_text.as_deref(), output).await
            }

            Commands::Start { id, output } => cmd_start(&id, output).await,
            Commands::Close {
                id,
                summary,
                no_summary,
                cancel,
                output,
            } => cmd_close(&id, summary.as_deref(), no_summary, cancel, output).await,
            Commands::Reopen { id, output } => cmd_reopen(&id, output).await,
            Commands::Status { id, status, output } => cmd_status(&id, status, output).await,
            Commands::Set {
                id,
                field,
                value,
                output,
            } => cmd_set(&id, &field, value.as_deref(), output).await,

            Commands::Dep { action } => match action {
                DepAction::Add { id, dep_id, output } => cmd_dep_add(&id, &dep_id, output).await,
                DepAction::Remove { id, dep_id, output } => {
                    cmd_dep_remove(&id, &dep_id, output).await
                }
                DepAction::Tree { id, full, output } => cmd_dep_tree(&id, full, output).await,
            },

            Commands::Link { action } => match action {
                LinkAction::Add { ids, output } => cmd_link_add(&ids, output).await,
                LinkAction::Remove { id1, id2, output } => {
                    cmd_link_remove(&id1, &id2, output).await
                }
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
                output,
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
                    output,
                };
                cmd_ls_with_options(opts).await
            }

            Commands::Query { filter } => cmd_query(filter.as_deref()).await,

            Commands::View => cmd_view().await,
            Commands::Board => cmd_board().await,

            Commands::Remote { action } => match action {
                RemoteAction::Browse { provider } => cmd_remote_browse(provider.as_deref()).await,
                RemoteAction::Adopt {
                    remote_ref,
                    prefix,
                    output,
                } => cmd_adopt(&remote_ref, prefix.as_deref(), output).await,
                RemoteAction::Push { id, output } => cmd_push(&id, output).await,
                RemoteAction::Link {
                    id,
                    remote_ref,
                    output,
                } => cmd_remote_link(&id, &remote_ref, output).await,
                RemoteAction::Sync { id, output } => cmd_sync(&id, output).await,
            },

            Commands::Config { action } => match action {
                ConfigAction::Show { output } => cmd_config_show(output),
                ConfigAction::Set { key, value, output } => cmd_config_set(&key, &value, output),
                ConfigAction::Get { key, output } => cmd_config_get(&key, output),
            },

            Commands::Cache { action } => match action {
                CacheAction::Status { output } => cmd_cache_status(output).await,
                CacheAction::Prune { output } => cmd_cache_prune(output).await,
                CacheAction::Rebuild { output } => cmd_cache_rebuild(output).await,
            },

            Commands::Events { action } => match action {
                EventsAction::Prune { output } => cmd_events_prune(output).await,
            },

            Commands::Hook { action } => match action {
                HookAction::List { output } => cmd_hook_list(output),
                HookAction::Install {
                    recipe,
                    force,
                    output,
                } => cmd_hook_install(&recipe, force, output).await,
                HookAction::Run { event, id } => cmd_hook_run(&event, id.as_deref()).await,
                HookAction::Enable { output } => cmd_hook_enable(output),
                HookAction::Disable { output } => cmd_hook_disable(output),
                HookAction::Log { lines, output } => cmd_hook_log(lines, output),
            },

            Commands::Doctor { output } => handle_validation_result(
                cmd_doctor(output),
                "Ticket health check failed - some files have errors",
            ),

            Commands::Plan { action } => match action {
                PlanAction::Create {
                    title,
                    phases,
                    output,
                } => cmd_plan_create(&title, &phases, output),
                PlanAction::Show {
                    id,
                    raw,
                    tickets_only,
                    phases_only,
                    verbose_phases,
                    output,
                } => {
                    cmd_plan_show(&id, raw, tickets_only, phases_only, &verbose_phases, output)
                        .await
                }
                PlanAction::Edit { id, output } => cmd_plan_edit(&id, output).await,
                PlanAction::Ls { status, output } => cmd_plan_ls(status, output).await,
                PlanAction::AddTicket {
                    plan_id,
                    ticket_id,
                    phase,
                    after,
                    position,
                    output,
                } => {
                    cmd_plan_add_ticket(
                        &plan_id,
                        &ticket_id,
                        phase.as_deref(),
                        after.as_deref(),
                        position,
                        output,
                    )
                    .await
                }
                PlanAction::RemoveTicket {
                    plan_id,
                    ticket_id,
                    output,
                } => cmd_plan_remove_ticket(&plan_id, &ticket_id, output).await,
                PlanAction::MoveTicket {
                    plan_id,
                    ticket_id,
                    to_phase,
                    after,
                    position,
                    output,
                } => {
                    cmd_plan_move_ticket(
                        &plan_id,
                        &ticket_id,
                        &to_phase,
                        after.as_deref(),
                        position,
                        output,
                    )
                    .await
                }
                PlanAction::AddPhase {
                    plan_id,
                    phase_name,
                    after,
                    position,
                    output,
                } => {
                    cmd_plan_add_phase(&plan_id, &phase_name, after.as_deref(), position, output)
                        .await
                }
                PlanAction::RemovePhase {
                    plan_id,
                    phase,
                    force,
                    migrate,
                    output,
                } => {
                    cmd_plan_remove_phase(&plan_id, &phase, force, migrate.as_deref(), output).await
                }
                PlanAction::Reorder {
                    plan_id,
                    phase,
                    reorder_phases,
                    output,
                } => cmd_plan_reorder(&plan_id, phase.as_deref(), reorder_phases, output).await,
                PlanAction::Delete { id, force, output } => {
                    cmd_plan_delete(&id, force, output).await
                }
                PlanAction::Rename {
                    id,
                    new_title,
                    output,
                } => cmd_plan_rename(&id, &new_title, output).await,
                PlanAction::Next {
                    id,
                    phase,
                    all,
                    count,
                    output,
                } => cmd_plan_next(&id, phase, all, count, output).await,
                PlanAction::Status { id, output } => cmd_plan_status(&id, output).await,
                PlanAction::Import {
                    file,
                    dry_run,
                    title,
                    ticket_type,
                    prefix,
                    output,
                } => {
                    cmd_plan_import(
                        &file,
                        dry_run,
                        title.as_deref(),
                        ticket_type,
                        prefix.as_deref(),
                        output,
                    )
                    .await
                }
                PlanAction::ImportSpec => cmd_show_import_spec(),
                PlanAction::Verify { output } => handle_validation_result(
                    cmd_plan_verify(output),
                    "Plan verification failed - some files have errors",
                ),
            },

            Commands::Graph {
                deps,
                spawn,
                all: _,
                format,
                root,
                plan,
                output,
            } => {
                cmd_graph(
                    deps,
                    spawn,
                    &format,
                    root.as_deref(),
                    plan.as_deref(),
                    output,
                )
                .await
            }

            Commands::Next { limit, output } => cmd_next(limit, output).await,

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
                output,
            } => cmd_search(&query, limit, threshold, output).await,
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
        TicketPriority::ALL_STRINGS,
    )
}

fn parse_type(s: &str) -> Result<TicketType, String> {
    parse_with_validation(
        s,
        |v| v.parse().map_err(|_| String::new()),
        "type",
        TicketType::ALL_STRINGS,
    )
}

fn parse_status(s: &str) -> Result<TicketStatus, String> {
    parse_with_validation(
        s,
        |v| TicketStatus::from_str(v).map_err(|_| String::new()),
        "status",
        TicketStatus::ALL_STRINGS,
    )
}

fn parse_partial_id(s: &str) -> Result<String, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err("ID cannot be empty".to_string());
    }
    if trimmed.starts_with('-') {
        return Err("ID cannot start with hyphen".to_string());
    }
    if !trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "ID '{trimmed}' contains invalid characters. Use only letters, numbers, hyphens, and underscores"
        ));
    }
    Ok(trimmed.to_string())
}

fn parse_plan_id(s: &str) -> Result<String, String> {
    // Character-level validation only - allows partial plan IDs
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err("Plan ID cannot be empty".to_string());
    }
    if !trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "Plan ID '{trimmed}' contains invalid characters. Use only letters, numbers, hyphens, and underscores"
        ));
    }
    Ok(trimmed.to_string())
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

fn parse_sort_field(s: &str) -> Result<SortField, String> {
    parse_with_validation(
        s,
        |v| v.parse().map_err(|_| String::new()),
        "sort field",
        SortField::ALL_STRINGS,
    )
}

fn parse_size(s: &str) -> Result<TicketSize, String> {
    let mut valid_values = TicketSize::ALL_STRINGS.to_vec();
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
