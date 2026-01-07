#!/bin/bash

set -euo pipefail

VERSION=${1:-$(git describe --tags --always)}
TARGETS=("aarch64-apple-darwin")

echo "Building Janus version: $VERSION"

for target in "${TARGETS[@]}"; do
    echo "Building for $target..."
    
    # Install target if not present
    rustup target add "$target"
    
    # Build optimized binary
    cargo build --release --target "$target"
    
    # Strip binary (if available)
    if command -v strip &> /dev/null; then
        strip "target/$target/release/janus" || true
    fi
    
    # Create release archive
    cd "target/$target/release"
    tar -czf "janus-$VERSION-$target.tar.gz" janus
    shasum -a 256 "janus-$VERSION-$target.tar.gz" > "janus-$VERSION-$target.tar.gz.sha256"
    
    echo "Created: janus-$VERSION-$target.tar.gz"
    cd - > /dev/null
    
    # Display file size
    ls -lh "target/$target/release/janus-$VERSION-$target.tar.gz"
done

echo "Build complete!"
