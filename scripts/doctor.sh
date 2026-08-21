#!/bin/bash

cd "$(dirname "$0")/.."

echo "=== Development Environment Check ==="
echo ""

# Check Rust
echo -n "Rust: "
if command -v rustc &> /dev/null; then
    rustc --version
else
    echo "❌ Not installed"
fi

# Check Cargo
echo -n "Cargo: "
if command -v cargo &> /dev/null; then
    cargo --version
else
    echo "❌ Not installed"
fi

# Check cargo-deb
echo -n "cargo-deb: "
if cargo deb --version &> /dev/null; then
    cargo deb --version
else
    echo "❌ Not installed (install with: cargo install cargo-deb)"
fi

# Check cargo-generate-rpm
echo -n "cargo-generate-rpm: "
if cargo generate-rpm --version &> /dev/null; then
    cargo generate-rpm --version
else
    echo "❌ Not installed (install with: cargo install cargo-generate-rpm)"
fi

# Check GTK4
echo -n "GTK4: "
if pkg-config --modversion gtk4 &> /dev/null; then
    pkg-config --modversion gtk4
else
    echo "❌ Not installed"
fi

# Check libadwaita
echo -n "libadwaita: "
if pkg-config --modversion libadwaita-1 &> /dev/null; then
    pkg-config --modversion libadwaita-1
else
    echo "❌ Not installed"
fi

echo ""
echo "=== Project Status ==="
echo -n "Release binary: "
if [ -f "target/release/data-cleaner" ]; then
    ls -lh target/release/data-cleaner | awk '{print $5}'
else
    echo "Not built"
fi

echo ""
