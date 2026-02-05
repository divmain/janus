# Remote Sync

Janus supports bidirectional synchronization with GitHub Issues and Linear. This allows you to:

- **Adopt** existing remote issues as local tickets
- **Push** new local tickets to create remote issues
- **Sync** bidirectional changes between local and remote

## GitHub Setup

### 1. Get a Personal Access Token

1. Go to https://github.com/settings/tokens
2. Create a new token with `repo` scope
3. Copy the token (starts with `ghp_`)

### 2. Configure Janus

```bash
# Method 1: Set directly
janus config set github.token ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx

# Method 2: Use environment variable (recommended for security)
export GITHUB_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx

# Set default GitHub repository
janus config set default_remote github:myorg/myrepo
```

### 3. Start Syncing

```bash
# Create a new local ticket
janus create "Add OAuth flow" --description "Implement Google OAuth login"

# Push it to GitHub (creates a new issue)
janus remote push j-a1b2

# Or adopt an existing GitHub issue
janus remote adopt github:myorg/myrepo/123

# Sync changes between local and remote
janus remote sync j-a1b2
```

## Linear Setup

### 1. Get an API Key

1. Go to https://linear.app/socketdev/settings/account/security
2. Create a personal API key
3. Copy the key (starts with `lin_api_`)

### 2. Configure Janus

```bash
# Method 1: Set directly
janus config set linear.api_key lin_api_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx

# Method 2: Use environment variable (recommended for security)
export LINEAR_API_KEY=lin_api_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx

# Set default Linear organization
janus config set default_remote linear:myorg
```

### 3. Start Syncing

```bash
# Create a new local ticket
janus create "Add user dashboard" --description "Build dashboard UI"

# Push it to Linear (creates a new issue)
janus remote push j-a1b2

# Or adopt an existing Linear issue
janus remote adopt linear:myorg/PROJ-123

# Sync changes between local and remote
janus remote sync j-a1b2
```

## Sync Workflows

### Creating Issues Remotely

```bash
# Option 1: Push a local ticket to create a new remote issue
janus create "Fix authentication bug" --priority 1
janus remote push j-abc1
# Creates a new issue on GitHub/Linear and links it

# Option 2: Link an existing local ticket to an existing remote issue
janus create "Update API docs"
janus remote link j-abc2 github:myorg/myrepo/456
```

### Adopting Existing Issues

```bash
# Adopt an existing GitHub issue as a local ticket
janus remote adopt github:facebook/react/1234

# Adopt an existing Linear issue
janus remote adopt linear:mycompany/ENG-456
```

Both commands create a local ticket with:
- Remote reference stored in `remote:` field
- Title, description, status, priority imported
- URL displayed for easy reference

### Bi-Directional Sync

When local and remote get out of sync:

```bash
janus remote sync j-abc1

# For each field that differs, you'll be prompted:
# [l]ocal->remote  - push local changes to remote
# [r]emote->local  - pull remote changes to local
# [s]kip           - keep them different
```

Sync currently supports:
- **Title**: Update title on either side
- **Status**: Sync status (with mapping for Linear's custom workflows)
- **Body/Description**: Update content

## Remote Commands

### `janus remote`

Manage remote issues. Without a subcommand in an interactive terminal, launches the TUI browser.

```bash
janus remote [COMMAND]

Commands:
  browse  Browse remote issues in TUI
  adopt   Import a remote issue and create a local ticket
  push    Push a local ticket to create a remote issue
  link    Link a local ticket to an existing remote issue
  sync    Sync a local ticket with its remote issue
```

### `janus remote adopt`

Import a remote issue and create a local ticket.

```bash
janus remote adopt [OPTIONS] <REMOTE_REF>

Options:
      --prefix <PREFIX>  Custom prefix for ticket ID
      --json             Output as JSON

# Examples
janus remote adopt github:owner/repo/123
janus remote adopt linear:org/PROJ-123
```

### `janus remote push`

Push a local ticket to create a remote issue.

```bash
janus remote push [OPTIONS] <ID>

Options:
      --json   Output as JSON
```

### `janus remote link`

Link a local ticket to an existing remote issue.

```bash
janus remote link [OPTIONS] <ID> <REMOTE_REF>

Options:
      --json   Output as JSON

# Example
janus remote link j-a1b2 github:myorg/myrepo/456
```

### `janus remote sync`

Sync a local ticket with its remote issue.

```bash
janus remote sync [OPTIONS] <ID>

Options:
      --json   Output as JSON
```

### `janus remote browse`

Browse remote issues in TUI.

```bash
janus remote browse           # Browse default remote
janus remote browse github    # Browse GitHub issues
janus remote browse linear    # Browse Linear issues
```

## Viewing Configuration

Check your current remote sync setup:

```bash
janus config show
```

Output:
```
Configuration:

default_remote:
  platform: github
  org: myorg
  repo: myrepo

auth:
  github.token: configured
  linear.api_key: configured

Config file: `.janus/config.yaml`
```

## Using Multiple Platforms

You can configure both GitHub and Linear simultaneously:

```bash
# Set both tokens
janus config set github.token ghp_xxxxxxxxxxxx
janus config set linear.api_key lin_api_xxxxxxxxxxxx

# Use full references to avoid ambiguity
janus remote adopt github:myorg/repo/123
janus remote adopt linear:myorg/PROJ-456
```

The `default_remote` setting only affects `janus remote push` (where platform must be inferred). Always use full reference format for `janus remote adopt` and `janus remote link`.

## Tips

- Use environment variables for sensitive credentials (`GITHUB_TOKEN`, `LINEAR_API_KEY`) instead of storing in config files
- Run `janus remote sync` regularly when collaborating via GitHub/Linear to keep local and remote in sync
- Once `default_remote` is set, use short formats (e.g., `ENG-123` for Linear instead of `linear:org/ENG-123`)
