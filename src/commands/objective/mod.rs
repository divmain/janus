//! Objective command implementations
//!
//! This module implements objective commands:
//! - `objective create` - Create a new objective
//! - `objective show` - Show objective details with computed status
//! - `objective ls` - List objectives with computed statuses
//! - `objective edit` - Open objective in $EDITOR
//! - `objective delete` - Delete an objective
//! - `objective ref` - Manage satisfied-by references
//! - `objective add-note` - Add a timestamped note
//! - `objective add-criterion` - Add an acceptance criterion

mod add_criterion;
mod add_note;
mod create;
mod delete;
mod edit;
mod ls;
mod refs;
mod show;

pub use add_criterion::cmd_objective_add_criterion;
pub use add_note::cmd_objective_add_note;
pub use create::cmd_objective_create;
pub use delete::cmd_objective_delete;
pub use edit::cmd_objective_edit;
pub use ls::cmd_objective_ls;
pub use refs::{cmd_objective_ref_add, cmd_objective_ref_del, cmd_objective_ref_reset};
pub use show::cmd_objective_show;
