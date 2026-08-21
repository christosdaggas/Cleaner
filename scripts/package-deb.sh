#!/bin/bash
set -e

cd "$(dirname "$0")/.."

echo "=== Building DEB Package ==="

echo "Building release binary..."
cargo build --release

# Create dist directory
mkdir -p dist/deb

# Build DEB package
cargo deb

# Move to dist
mv target/debian/*.deb dist/deb/

echo ""
echo "✅ DEB package created!"
ls -lh dist/deb/*.deb
