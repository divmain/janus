#[macro_use]
pub mod macros;

pub mod cli;
pub mod commands;
pub mod config;
pub mod display;
pub mod doc;
pub mod entity;
pub mod error;
pub mod events;
pub mod fs;
pub mod graph;
pub mod hooks;
pub mod locator;
pub mod mcp;
pub mod next;
pub mod parser;
pub mod paths;
pub mod plan;
pub mod query;
pub mod remote;
pub mod status;
pub mod ticket;
pub mod tui;
pub mod types;
pub mod utils;

pub mod embedding;
pub mod store;

pub use config::Config;
pub use doc::types::{DocChunk, DocLabel, DocLoadResult, DocMetadata};
pub use doc::{
    Doc, chunk_document, ensure_docs_dir, get_all_docs_from_disk, parse_doc_content, serialize_doc,
};
pub use entity::Entity;
pub use error::{JanusError, Result};
pub use graph::{check_circular_dependency, resolve_id_from_map};
pub use hooks::{
    HookContext, HookEvent, context_to_env, run_post_hooks, run_post_hooks_async, run_pre_hooks,
    run_pre_hooks_async,
};
pub use plan::parser::parse_plan_content;
pub use plan::types::{FreeFormSection, Phase, PhaseStatus, PlanMetadata, PlanSection, PlanStatus};
pub use plan::{
    Plan, build_plan_map, compute_all_phase_statuses, compute_plan_status, ensure_plans_dir,
    generate_plan_id, get_all_plans, get_all_plans_with_map,
};
pub use remote::{Platform, RemoteIssue, RemoteRef, RemoteStatus};
pub use ticket::{
    Ticket, TicketBuilder, TicketLoadResult, build_ticket_map, get_all_tickets,
    get_all_tickets_from_disk,
};
pub use types::{
    ArrayField, CreatedAt, EntityType, PlanId, TicketData, TicketId, TicketMetadata,
    TicketPriority, TicketStatus, TicketSummary, TicketType, docs_dir, janus_root, plans_dir,
    tickets_items_dir,
};
