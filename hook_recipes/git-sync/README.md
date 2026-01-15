# Git Sync Recipe

Sync your `.janus/` directory with a Git remote for team collaboration.

## Setup

1. Install the recipe:
   ```bash
   janus hook install git-sync
   ```

2. Initialize git sync with your remote:
   ```bash
   .janus/hooks/setup.sh git@github.com:yourorg/yourrepo-janus.git
   ```

## Usage

After setup, changes are automatically committed and pushed after each Janus operation.

To manually sync (pull remote changes + push local):
```bash
.janus/hooks/sync.sh
```

## What gets synced

- `items/` - All tickets
- `plans/` - All plans

## What stays local

- `hooks/` - Hook scripts (each machine has its own)
- `config.yaml` - Local configuration (auth tokens, settings)
