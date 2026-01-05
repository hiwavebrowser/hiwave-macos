#!/bin/bash
# Capture golden images for all test fixtures
#
# Usage: ./scripts/capture_goldens.sh [fixture_name]
#
# If no fixture_name is provided, captures all fixtures.

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURES_DIR="$PROJECT_ROOT/fixtures"
GOLDENS_DIR="$PROJECT_ROOT/goldens"

# Create goldens directory if it doesn't exist
mkdir -p "$GOLDENS_DIR"

# Build release first
echo "Building release..."
cd "$PROJECT_ROOT"
cargo build --release -p hiwave-smoke 2>/dev/null || {
    echo "Warning: hiwave-smoke build failed, trying dev build..."
    cargo build -p hiwave-smoke
}

SMOKE_BIN="$PROJECT_ROOT/target/release/hiwave-smoke"
if [ ! -f "$SMOKE_BIN" ]; then
    SMOKE_BIN="$PROJECT_ROOT/target/debug/hiwave-smoke"
fi

# Function to capture a single fixture
capture_fixture() {
    local fixture_path="$1"
    local fixture_name=$(basename "$fixture_path" .html)
    local output_path="$GOLDENS_DIR/${fixture_name}.ppm"
    
    echo "Capturing: $fixture_name"
    
    # Run smoke harness with the fixture
    "$SMOKE_BIN" \
        --html-file "$fixture_path" \
        --duration-ms 1000 \
        --dump-frame "$output_path" \
        2>/dev/null || {
            echo "  Warning: Capture failed for $fixture_name"
            return 1
        }
    
    if [ -f "$output_path" ]; then
        local size=$(ls -lh "$output_path" | awk '{print $5}')
        echo "  Captured: $output_path ($size)"
        
        # Convert to PNG if ImageMagick is available
        if command -v convert &> /dev/null; then
            local png_path="$GOLDENS_DIR/${fixture_name}.png"
            convert "$output_path" "$png_path" 2>/dev/null && {
                echo "  Converted: $png_path"
            }
        fi
    else
        echo "  No output file created"
        return 1
    fi
}

# If a specific fixture is requested
if [ -n "$1" ]; then
    fixture_path="$FIXTURES_DIR/$1"
    if [ ! -f "$fixture_path" ]; then
        fixture_path="$FIXTURES_DIR/$1.html"
    fi
    
    if [ -f "$fixture_path" ]; then
        capture_fixture "$fixture_path"
    else
        echo "Fixture not found: $1"
        exit 1
    fi
else
    # Capture all fixtures
    echo "Capturing golden images for all fixtures..."
    echo ""
    
    captured=0
    failed=0
    
    for fixture in "$FIXTURES_DIR"/*.html; do
        if [ -f "$fixture" ]; then
            if capture_fixture "$fixture"; then
                ((captured++))
            else
                ((failed++))
            fi
            echo ""
        fi
    done
    
    echo "=============================="
    echo "Summary: $captured captured, $failed failed"
fi

echo ""
echo "Golden images stored in: $GOLDENS_DIR"

