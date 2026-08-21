#!/bin/bash
set -e

cd "$(dirname "$0")/.."

echo "=== Building RPM Package ==="

# Always rebuild release so the RPM packages current sources.
echo "Building release binary..."
cargo build --release --locked

# Create dist directory
mkdir -p dist/rpm

# Build RPM package
cargo generate-rpm

# Move to dist
mv target/generate-rpm/*.rpm dist/rpm/

echo ""
echo "✅ RPM package created!"
ls -lh dist/rpm/*.rpm
