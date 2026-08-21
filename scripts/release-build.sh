#!/bin/bash
set -e

cd "$(dirname "$0")/.."

echo "=== Building Data Cleaner (Release) ==="
cargo build --release --locked

echo ""
echo "✅ Release build complete!"
echo "Binary: target/release/data-cleaner"
ls -lh target/release/data-cleaner
