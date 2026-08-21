#!/bin/bash
set -e

cd "$(dirname "$0")/.."

echo "=== Cleaning Build Artifacts ==="

# Clean cargo build
cargo clean

# Clean dist folder
rm -rf dist

# Clean build folder
rm -rf build/appimage

echo "✅ Clean complete!"
