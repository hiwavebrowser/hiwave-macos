#!/bin/bash
# websuite_diff.sh - Full websuite visual diff workflow
#
# This script:
# 1. Captures RustKit frames
# 2. Captures Chromium baselines (if needed)
# 3. Compares and generates diff report
#
# Usage: ./scripts/websuite_diff.sh [--regenerate-baseline]

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BASELINE_TOOL="$PROJECT_DIR/tools/websuite-baseline"

REGENERATE_BASELINE=false
if [ "$1" = "--regenerate-baseline" ]; then
    REGENERATE_BASELINE=true
fi

echo "WebSuite Visual Diff Workflow"
echo "=============================="
echo ""

# Step 1: Capture RustKit frames
echo "Step 1: Capture RustKit frames"
echo "------------------------------"
"$SCRIPT_DIR/websuite_capture.sh"
echo ""

# Step 2: Ensure baselines exist
BASELINE_DIR="$PROJECT_DIR/websuite/baselines"
if [ ! -d "$BASELINE_DIR" ] || [ "$REGENERATE_BASELINE" = true ]; then
    echo "Step 2: Generate Chromium baselines"
    echo "------------------------------------"
    
    cd "$BASELINE_TOOL"
    
    # Install dependencies if needed
    if [ ! -d "node_modules" ]; then
        echo "Installing dependencies..."
        npm install
        npx playwright install chromium
    fi
    
    # Capture baselines
    node capture.js
    echo ""
else
    echo "Step 2: Using existing baselines (use --regenerate-baseline to refresh)"
    echo ""
fi

# Step 3: Compare
echo "Step 3: Compare RustKit vs Chromium"
echo "------------------------------------"
cd "$BASELINE_TOOL"

# Install pngjs if needed
if ! node -e "require('pngjs')" 2>/dev/null; then
    npm install pngjs
fi

node compare.js

echo ""
echo "Done! Check websuite/diffs/ for results."

