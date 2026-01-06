# External Issue Sync Implementation Plan

This document describes the implementation plan for synchronizing Janus issues with external issue trackers (GitHub Issues and Linear.app).

## Overview

The feature enables bidirectional synchronization between local Janus tickets and remote issue trackers. Users can adopt remote issues locally, push local issues to remote systems, link existing issues, and synchronize state changes.

## External Reference Format

Remote issues are referenced using a URI-like format stored in YAML frontmatter:

```yaml
remote: linear:myorg/PROJ-123
# or
remote: github:owner/repo/123
```

**Format breakdown:**
- `github:owner/repo/issue_number` - GitHub Issues
- `linear:org/ISSUE-ID` - Linear.app issues

**Short format (when default org is configured):**
- `PROJ-123` resolves to `linear:default-org/PROJ-123`
- `owner/repo/123` resolves to `github:owner/repo/123`

The `remote` field replaces the previous `external-ref` field.

## Configuration

A configuration file will be stored at `.janus/config.yaml`:

```yaml
# Default remote platform and organization
default_remote:
  platform: linear  # or "github"
  org: myorg        # Linear org or GitHub owner

# Authentication tokens (stored locally, not committed)
auth:
  github:
    token: ghp_xxxxxxxxxxxx
  linear:
    api_key: lin_api_xxxxxxxxxxxx
```

**Security note:** The config file containing tokens should be added to `.gitignore`.

## Commands

### `janus adopt <remote-ref>`

Fetch a remote issue and create a corresponding local ticket.

```bash
janus adopt linear:myorg/PROJ-123
janus adopt github:owner/repo/456
janus adopt PROJ-123  # Uses default org if configured
```

**Behavior:**
1. Parse remote reference (with default org fallback)
2. Fetch issue from remote API
3. Create local ticket with mapped fields
4. Set `remote` field in frontmatter
5. Print local ticket ID

### `janus push <local-id>`

Create a remote issue from a local ticket.

```bash
janus push j-a1b2
```

**Behavior:**
1. Find local ticket by ID
2. Verify no `remote` field exists (error if already linked)
3. Create issue on remote platform (using default remote config)
4. Update local ticket's `remote` field
5. Print remote issue reference

**Output example:**
```
Created linear:myorg/PROJ-456
Updated j-a1b2 -> remote: linear:myorg/PROJ-456
```

### `janus link <local-id> <remote-ref>`

Link an existing local ticket to an existing remote issue.

```bash
janus link j-a1b2 linear:myorg/PROJ-123
janus link j-a1b2 github:owner/repo/456
```

**Behavior:**
1. Find local ticket by ID
2. Parse and validate remote reference
3. Optionally verify remote issue exists (API call)
4. Set `remote` field in frontmatter
5. Print confirmation

### `janus sync [<local-id>]`

Synchronize state between local and remote issues.

```bash
janus sync j-a1b2  # Sync specific ticket
janus sync         # Sync all linked tickets (future enhancement)
```

**Behavior:**
1. Find local ticket and its remote reference
2. Fetch current state from both local and remote
3. Compare timestamps and detect changes in:
   - Status
   - Title
   - Description/body
   - Priority (if supported)
   - Assignee (if supported)
4. For each difference, prompt user:
   ```
   Status differs:
     Local:  complete (updated 2024-01-15T10:00:00Z)
     Remote: open (updated 2024-01-15T09:00:00Z)
   
   Sync? [l]ocal->remote, [r]emote->local, [s]kip: 
   ```
5. Apply selected changes
6. Update timestamps

## Architecture

### Directory Structure

```
src/
├── remote/
│   ├── mod.rs           # RemoteRef parsing, Provider trait, config
│   ├── config.rs        # Configuration file handling
│   ├── github.rs        # GitHub provider implementation
│   └── linear.rs        # Linear provider implementation
├── commands/
│   ├── sync.rs          # adopt, push, sync commands
│   └── ...
```

### Core Types

```rust
// src/remote/mod.rs

/// Parsed remote reference
#[derive(Debug, Clone, PartialEq)]
pub enum RemoteRef {
    GitHub { owner: String, repo: String, issue_number: u64 },
    Linear { org: String, issue_id: String },
}

impl RemoteRef {
    /// Parse from string like "github:owner/repo/123" or "linear:org/PROJ-123"
    pub fn parse(s: &str, config: Option<&Config>) -> Result<Self>;
    
    /// Serialize back to string format
    pub fn to_string(&self) -> String;
}

/// Normalized remote issue data
pub struct RemoteIssue {
    pub id: String,
    pub title: String,
    pub body: String,
    pub status: RemoteStatus,
    pub priority: Option<u8>,
    pub assignee: Option<String>,
    pub updated_at: DateTime,
    pub url: String,
}

/// Platform-agnostic status
pub enum RemoteStatus {
    Open,
    Closed,
    Custom(String),  // For Linear's custom workflow states
}

/// Common interface for remote providers
#[async_trait]
pub trait RemoteProvider {
    async fn fetch_issue(&self, remote_ref: &RemoteRef) -> Result<RemoteIssue>;
    async fn create_issue(&self, title: &str, body: &str) -> Result<RemoteRef>;
    async fn update_issue(&self, remote_ref: &RemoteRef, updates: IssueUpdates) -> Result<()>;
}
```

### Configuration Types

```rust
// src/remote/config.rs

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub default_remote: Option<DefaultRemote>,
    pub auth: AuthConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DefaultRemote {
    pub platform: Platform,
    pub org: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Platform {
    GitHub,
    Linear,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthConfig {
    pub github: Option<GitHubAuth>,
    pub linear: Option<LinearAuth>,
}
```

## Dependencies

### Required Crates

```toml
[dependencies]
# Async runtime (required for HTTP clients)
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }

# GitHub API client
octocrab = "0.48"

# HTTP client for Linear API
reqwest = { version = "0.12", features = ["json"] }

# GraphQL support for Linear
# TODO: Evaluate graphql_client or cynic for type-safe GraphQL
```

### Linear API Integration

**Important:** The `linear_sdk` crate is outdated (4+ years old, v0.0.1) and should not be used.

Linear provides a GraphQL API. The recommended approach is to:

1. **Phase 1 (MVP):** Use `reqwest` with hand-crafted GraphQL queries
2. **Phase 2 (Type-Safe Client):** Generate a type-safe client from Linear's GraphQL schema

#### GraphQL Library Recommendation: `cynic`

After evaluating available options, **`cynic`** is recommended over `graphql_client`:

| Feature | cynic | graphql_client |
|---------|-------|----------------|
| Schema validation | Compile-time | Compile-time |
| Query definition | Rust structs with derives | Separate `.graphql` files |
| Code generation | Online generator + build.rs | CLI tool |
| Large schema handling | Optimized (important for Linear) | Can be slow |
| Learning curve | Moderate | Lower |

**Why cynic:**
- Linear's schema is large; cynic has specific optimizations for large APIs
- Rust-native query definitions (no separate `.graphql` files)
- Online generator at https://generator.cynic-rs.dev/ for quick prototyping
- Active maintenance

#### Linear GraphQL Schema

The official Linear GraphQL schema is available at:
https://github.com/linear/linear/blob/master/packages/sdk/src/schema.graphql

To set up cynic:
1. Download the schema: `cynic-cli introspect https://api.linear.app/graphql -H "Authorization: <token>" > schema.graphql`
2. Or fetch directly from GitHub (no auth required for schema file)
3. Register in `build.rs` with `cynic_codegen::register_schema`

#### Phase 2 Dependencies

```toml
[dependencies]
cynic = { version = "3", features = ["http-reqwest"] }

[build-dependencies]
cynic-codegen = "3"
```

## Status Mapping

| Janus Status | GitHub State | Linear State |
|--------------|--------------|--------------|
| `new` | `open` | Backlog/Todo* |
| `complete` | `closed` | Done* |
| `cancelled` | `closed` | Cancelled* |

*Linear's workflow states are team-specific. Initial implementation will use best-effort mapping based on state names.

## Implementation Phases

### Phase 1: Core Infrastructure
- [ ] Add `remote` field to `TicketMetadata` (replace `external-ref`)
- [ ] Create `src/remote/mod.rs` with `RemoteRef` parsing
- [ ] Create `src/remote/config.rs` for configuration handling
- [ ] Add new error variants to `error.rs`
- [ ] Add async runtime dependency (`tokio`)

### Phase 2: GitHub Provider
- [ ] Create `src/remote/github.rs`
- [ ] Implement authentication via config file
- [ ] Implement `fetch_issue` using `octocrab`
- [ ] Implement `create_issue` using `octocrab`
- [ ] Implement `update_issue` for sync

### Phase 3: Linear Provider (MVP)
- [ ] Create `src/remote/linear.rs`
- [ ] Implement authentication via config file
- [ ] Implement `fetch_issue` via raw GraphQL query
- [ ] Implement `create_issue` via GraphQL mutation
- [ ] Implement `update_issue` for sync

### Phase 4: Commands
- [ ] Create `src/commands/sync.rs`
- [ ] Implement `cmd_adopt`
- [ ] Implement `cmd_push`
- [ ] Update `cmd_link` to support remote references
- [ ] Implement `cmd_sync` with interactive prompts

### Phase 5: Configuration Commands
- [ ] `janus config set github.token <token>`
- [ ] `janus config set linear.api_key <key>`
- [ ] `janus config set default_remote <platform:org>`
- [ ] `janus config show`

### Phase 6: Testing & Polish
- [ ] Unit tests for `RemoteRef` parsing
- [ ] Integration tests with mocked API responses
- [ ] Add `--dry-run` flag for `push` and `sync`
- [ ] Documentation in README

## Future Enhancements

- **Automatic sync:** Watch for changes and sync automatically
- **Webhooks:** Receive push notifications from remote platforms
- **Batch operations:** `janus sync --all` to sync all linked tickets
- **Type-safe Linear client:** Generate from GraphQL schema
- **OAuth flow:** For Linear, support OAuth2 instead of just API keys
- **Multiple remotes per project:** Allow different tickets to link to different platforms

## Security Considerations

1. **Token storage:** Auth tokens are stored in `.janus/config.yaml`
2. **Git ignore:** The config file should be in `.gitignore` by default
3. **Environment variables:** As fallback, support `GITHUB_TOKEN` and `LINEAR_API_KEY`
4. **Token scopes:** Document minimum required permissions for each platform
