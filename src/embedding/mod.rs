//! Embedding module for semantic search
//!
//! This module provides functionality for generating text embeddings
//! and performing semantic search over ticket content.
//!
//! Enabled via the `semantic-search` feature flag.

pub mod model;
pub mod search;

pub use model::*;
pub use search::*;
