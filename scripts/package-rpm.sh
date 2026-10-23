#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

echo "=== Building RPM Package ==="

# Always rebuild release so the RPM packages current sources.
echo "Building release binary..."
cargo build --release --locked

# Create dist directory
mkdir -p dist/rpm

# Write the RPM directly to the project's distribution directory.
cargo generate-rpm --output dist/rpm/

echo ""
echo "✅ RPM package created!"
ls -lh dist/rpm/*.rpm
