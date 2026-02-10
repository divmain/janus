//! ID validation utilities
//!
//! This module provides shared validation logic for entity IDs (tickets, plans, etc.)
//! to prevent code duplication across modules. The validation ensures IDs are safe
//! for filesystem use and contain only valid characters.

// Note: validate_safe_id has been removed as its defense-in-depth checks were
// provably unreachable (validate_identifier already rejects '/', '\', and '.').
// Use validate_identifier directly from utils module instead.
