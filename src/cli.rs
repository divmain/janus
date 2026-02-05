use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use std::io;
use std::str::FromStr;

use crate::types::{
    DEFAULT_PRIORITY_STR, TicketPriority, TicketSize, TicketStatus, TicketType, VALID_PRIORITIES,
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
        status: String,

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
        #[arg(long, conflicts_with_all = ["ready", "blocked", "closed", "active"])]
        status: Option<String>,

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
        #[arg(long)]
        triaged: Option<String>,

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

        /// Show both deps and spawning relationships (default)
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
        #[arg(long)]
        status: Option<String>,

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
    TicketStatus::from_str(s)
        .map(|_| s.to_string())
        .map_err(|_| {
            format!(
                "Invalid status. Must be one of: {}",
                VALID_STATUSES.join(", ")
            )
        })
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

fn parse_size(s: &str) -> Result<TicketSize, String> {
    s.parse().map_err(|_| {
        format!(
            "Invalid size. Must be one of: {} (or aliases: xs, s, m, l, xl)",
            VALID_SIZES.join(", ")
        )
    })
}

pub fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "janus", &mut io::stdout());
}
