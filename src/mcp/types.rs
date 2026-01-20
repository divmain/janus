//! MCP-specific types for Janus.
//!
//! This module contains types used by the MCP server implementation.
//! Most MCP types are provided by the rmcp crate, but this module
//! can contain Janus-specific extensions if needed.

/// MCP protocol version supported by this server.
///
/// This matches the protocol version from the MCP specification.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Server name as reported during MCP initialization.
pub const SERVER_NAME: &str = "janus";

/// Server version as reported during MCP initialization.
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
