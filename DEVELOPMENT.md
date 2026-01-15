# Development Guide

This document covers development workflows for Janus, including building, testing, and releasing.

## Additional Developer Resources

See [AGENTS.md](AGENTS.md) for detailed information on:
- Project overview and technology stack
- Comprehensive build, test, and lint commands
- Project structure and file organization
- Code style guidelines and conventions
- Caching architecture and implementation
- Domain concepts (tickets, plans, statuses, etc.)
- Common patterns for working with tickets and plans

## Prerequisites

- Rust toolchain (stable)
- macOS for release builds (ARM64)

## Building

For detailed information on building, testing, and linting commands, see [AGENTS.md](AGENTS.md).

Basic commands:
```bash
# Debug build
cargo build

# Release build
cargo build --release
```

## Local Development

```bash
# Run from source
cargo run -- <command>

# Example: list tickets
cargo run -- ls

# Run with debug logging
RUST_LOG=debug cargo run -- ls
```

## Creating a Release

Releases are automated via GitHub Actions. When you push a version tag, the CI will:

1. Run tests
2. Build an optimized binary for `aarch64-apple-darwin`
3. Create a GitHub Release with the binary tarball
4. Trigger the Homebrew tap update at `divmain/homebrew-janus`

### Release Steps

1. **Ensure all changes are committed and pushed to main**

2. **Update version in `Cargo.toml`** (if not already done):
   ```toml
   [package]
   version = "X.Y.Z"
   ```

3. **Create and push a version tag**:
   ```bash
   # Create annotated tag
   git tag -a v1.0.0 -m "Release v1.0.0"
   
   # Push the tag
   git push origin v1.0.0
   ```

4. **Monitor the release**:
   - Go to https://github.com/divmain/janus/actions to watch the build
   - Once complete, the release will appear at https://github.com/divmain/janus/releases
   - The Homebrew tap will be automatically updated

5. **Verify the Homebrew release** (after ~2-3 minutes):
   ```bash
   # Update Homebrew
   brew update
   
   # Check the new version
   brew info divmain/janus/janus
   
   # Upgrade if already installed
   brew upgrade janus
   ```

### Local Release Build

For testing release builds locally without pushing:

```bash
# Build release binary
./scripts/build-release.sh v1.0.0

# Output will be at:
# target/aarch64-apple-darwin/release/janus-v1.0.0-aarch64-apple-darwin.tar.gz
```

## CI/CD Architecture

### Release Workflow (`.github/workflows/release.yml`)

Triggered on version tags (`v*`):

1. **Build Job** (macOS):
   - Installs Rust with ARM64 target
   - Runs `cargo test`
   - Builds optimized release binary
   - Strips debug symbols
   - Creates tarball and uploads to GitHub Releases

2. **Trigger Homebrew Update Job**:
   - Sends `repository_dispatch` event to `divmain/homebrew-janus`
   - Passes version info in payload

### Homebrew Tap Update (`divmain/homebrew-janus`)

When triggered:
1. Downloads the release tarball from `divmain/janus`
2. Re-uploads to `divmain/homebrew-janus` releases (public proxy)
3. Calculates SHA256 checksum
4. Updates `Casks/janus.rb` with new version and checksum
5. Commits and pushes changes

## Required GitHub Secrets

| Secret | Repository | Purpose |
|--------|-----------|---------|
| `PERSONAL_ACCESS_TOKEN` | `divmain/janus` | Trigger dispatch to homebrew-janus |
| `PERSONAL_ACCESS_TOKEN` | `divmain/homebrew-janus` | Download assets from janus releases |

The token needs `repo` scope for:
- Creating releases
- Triggering repository dispatch events
- Downloading release assets

### Creating the Personal Access Token

1. Go to https://github.com/settings/tokens
2. Click "Generate new token (classic)"
3. Select the `repo` scope (full control of private repositories)
4. Copy the token
5. Add it as a secret named `PERSONAL_ACCESS_TOKEN` in both repositories:
   - https://github.com/divmain/janus/settings/secrets/actions
   - https://github.com/divmain/homebrew-janus/settings/secrets/actions

## Version Numbering

Follow [Semantic Versioning](https://semver.org/):

- **MAJOR** (`X.0.0`): Breaking changes
- **MINOR** (`0.X.0`): New features, backwards compatible
- **PATCH** (`0.0.X`): Bug fixes, backwards compatible

Always prefix tags with `v` (e.g., `v1.0.0`, `v1.2.3`).

## Troubleshooting Releases

### Build Fails

Check the Actions log at https://github.com/divmain/janus/actions

Common issues:
- Test failures: Run `cargo test` locally to debug
- Compilation errors: Check for platform-specific code issues

### Homebrew Tap Not Updated

1. Check if the dispatch was triggered:
   - Go to https://github.com/divmain/homebrew-janus/actions
   - Look for "Update Cask" workflow runs

2. If no run appeared:
   - Verify `PERSONAL_ACCESS_TOKEN` is set in `divmain/janus`
   - Check the token has `repo` scope

3. If the run failed:
   - Check workflow logs for errors
   - Verify `PERSONAL_ACCESS_TOKEN` is set in `divmain/homebrew-janus`

### Manual Homebrew Update

If automation fails, you can manually trigger the update:

1. Go to https://github.com/divmain/homebrew-janus/actions
2. Select "Update Cask" workflow
3. Click "Run workflow"
4. Enter the version (e.g., `v1.0.0`)
5. Click "Run workflow"
