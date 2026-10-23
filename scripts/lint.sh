#!/bin/bash
set -e

cd "$(dirname "$0")/.."

echo "=== Running Lints ==="

# Check formatting
echo "Checking formatting..."
cargo fmt --check

# Run clippy
echo ""
echo "Running Clippy..."
cargo clippy --all-targets -- -D warnings

echo ""
echo "✅ All lints passed!"
