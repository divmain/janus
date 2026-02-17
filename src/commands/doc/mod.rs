//! Document command implementations
//!
//! This module implements document commands:
//! - `doc ls` - List all documents
//! - `doc show` - Display a document
//! - `doc create` - Create a new document
//! - `doc edit` - Edit a document
//! - `doc search` - Search documents semantically

mod create;
mod edit;
mod ls;
mod search;
mod show;

pub use create::cmd_doc_create;
pub use edit::cmd_doc_edit;
pub use ls::cmd_doc_ls;
pub use search::cmd_doc_search;
pub use show::cmd_doc_show;
