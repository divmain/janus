//! Embedding module for semantic search
//!
//! This module provides functionality for generating text embeddings
//! and performing semantic search over ticket content.

pub mod model;
pub mod search;

pub use model::*;
pub use search::*;
