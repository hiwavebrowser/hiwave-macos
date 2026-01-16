#!/bin/bash
# run-rustkit.sh - Run HiWave with RustKit engine (default)
#
# This script builds and runs HiWave using the RustKit rendering engine
# for content display, with WRY (WebKit) for Chrome UI and Shelf components.
#
# Features:
# - RustKit: Pure Rust browser engine for content rendering
# - Engine-level ad blocking via shield adapter
# - Hardware-accelerated GPU rendering via wgpu
#
# Usage:
#   ./scripts/run-rustkit.sh [cargo-args...]
#
# Examples:
#   ./scripts/run-rustkit.sh           # Build and run (debug)
#   ./scripts/run-rustkit.sh --release # Build and run (release)

set -e

cd "$(dirname "$0")/.."

echo "Building and running HiWave with RustKit engine..."
cargo run -p hiwave-app --features rustkit "$@"

