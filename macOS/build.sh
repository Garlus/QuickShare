#!/bin/bash
set -euo pipefail

# Build script for macOS QuickShare
# Compiles Rust core as a universal static library and sets up Xcode integration

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/.."
CORE_DIR="$PROJECT_DIR/core"

echo "=== Building quickshare-core for macOS ==="

# Ensure rustup target is installed
rustup target add aarch64-apple-darwin 2>/dev/null || true

# Build Rust static library
cd "$CORE_DIR"
cargo build --release --target aarch64-apple-darwin

# Copy static library to macOS bridge directory
cp "$CORE_DIR/target/aarch64-apple-darwin/release/libquickshare_core.a" \
   "$SCRIPT_DIR/Bridge/libquickshare_core.a"

echo "=== Rust static library built and copied ==="
echo "Output: macOS/Bridge/libquickshare_core.a"
