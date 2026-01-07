//! GraphQL schema definitions for Janus.
//!
//! This crate contains the generated schema types for GraphQL APIs used by Janus.
//! Separating these into their own crate improves compile times by avoiding
//! recompilation when unrelated code changes.

// Disable all clippy lints for this crate - it's entirely generated code
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]

/// Linear.app GraphQL schema types.
///
/// This module is generated from the Linear GraphQL schema and exports
/// all the types needed for constructing type-safe GraphQL queries.
#[cynic::schema("linear")]
pub mod linear {}
