#!/bin/bash
# HiWave RustKit Development Environment Initialization

set -e

echo "🚀 Initializing HiWave development environment..."

# Check Rust toolchain
if ! command -v rustc &> /dev/null; then
    echo "❌ Rust not found. Installing..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

echo "✅ Rust $(rustc --version)"

# Check Python
if ! command -v python3 &> /dev/null; then
    echo "❌ Python3 not found. Please install Python 3.11+"
    exit 1
fi

echo "✅ Python $(python3 --version)"

# Install Python dependencies for parity testing
if [ -f "scripts/requirements.txt" ]; then
    echo "📦 Installing Python dependencies..."
    python3 -m pip install -q -r scripts/requirements.txt
fi

# Check for Chrome/Chromium (needed for screenshots)
if command -v google-chrome &> /dev/null; then
    echo "✅ Chrome found"
elif command -v chromium &> /dev/null; then
    echo "✅ Chromium found"
else
    echo "⚠️  Chrome/Chromium not found - parity tests may fail"
fi

# Build RustKit (initial build to verify everything works)
echo "🔨 Building RustKit..."
cargo build --release 2>&1 | tail -5

echo ""
echo "✅ Environment ready!"
echo "📝 To run parity tests manually: python3 scripts/parity_test.py --scope all"
echo "🤖 To start Ralph: ./ralph.sh"
