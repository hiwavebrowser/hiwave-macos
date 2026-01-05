#!/bin/bash
# run-webkit.sh - Run HiWave with WebKit fallback (no RustKit)
#
# This script builds and runs HiWave using system WebKit for all
# rendering, bypassing the RustKit engine entirely.
#
# Features:
# - WebKit: Apple's battle-tested browser engine for all rendering
# - WRY framework for WebView management
# - Full compatibility with macOS system features
#
# Use this mode when:
# - Debugging issues that might be RustKit-specific
# - Testing compatibility with system WebKit
# - Comparing rendering behavior between engines
#
# Usage:
#   ./scripts/run-webkit.sh [cargo-args...]
#
# Examples:
#   ./scripts/run-webkit.sh           # Build and run (debug)
#   ./scripts/run-webkit.sh --release # Build and run (release)

set -e

cd "$(dirname "$0")/.."

echo "Building HiWave with WebKit fallback..."
cargo build -p hiwave-app --no-default-features --features webview-fallback "$@"

echo "Running HiWave (WebKit fallback mode)..."
cargo run -p hiwave-app --no-default-features --features webview-fallback "$@"

