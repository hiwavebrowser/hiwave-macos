#!/bin/bash
# Visual regression test runner
#
# Compares current rendering output against golden images.
# Reports any differences and generates diff images.
#
# Usage: ./scripts/visual_regression.sh [fixture_name]

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURES_DIR="$PROJECT_ROOT/fixtures"
GOLDENS_DIR="$PROJECT_ROOT/goldens"
RESULTS_DIR="$PROJECT_ROOT/.ai/artifacts/regression_$(date +%Y%m%dT%H%M%S)"

# Create results directory
mkdir -p "$RESULTS_DIR"

# Build
echo "Building..."
cd "$PROJECT_ROOT"
cargo build --release -p hiwave-smoke 2>/dev/null || cargo build -p hiwave-smoke

SMOKE_BIN="$PROJECT_ROOT/target/release/hiwave-smoke"
if [ ! -f "$SMOKE_BIN" ]; then
    SMOKE_BIN="$PROJECT_ROOT/target/debug/hiwave-smoke"
fi

# Tolerance for pixel comparison (0-255)
TOLERANCE=5

# Function to compare two PPM images using Python
compare_images() {
    local golden="$1"
    local current="$2"
    local diff_output="$3"
    
    python3 << EOF
import sys

def read_ppm(path):
    with open(path, 'rb') as f:
        # Read header
        magic = f.readline().decode().strip()
        if magic != 'P6':
            return None, 0, 0
        
        # Skip comments
        line = f.readline().decode()
        while line.startswith('#'):
            line = f.readline().decode()
        
        dims = line.strip().split()
        width, height = int(dims[0]), int(dims[1])
        max_val = int(f.readline().decode().strip())
        
        # Read pixel data
        data = f.read()
        return data, width, height

try:
    golden_data, gw, gh = read_ppm('$golden')
    current_data, cw, ch = read_ppm('$current')
    
    if golden_data is None or current_data is None:
        print("ERROR: Failed to read images")
        sys.exit(2)
    
    if (gw, gh) != (cw, ch):
        print(f"SIZE_MISMATCH: golden={gw}x{gh}, current={cw}x{ch}")
        sys.exit(1)
    
    # Compare pixels
    diff_count = 0
    tolerance = $TOLERANCE
    
    for i in range(0, len(golden_data), 3):
        if i + 2 >= len(golden_data) or i + 2 >= len(current_data):
            break
        
        dr = abs(golden_data[i] - current_data[i])
        dg = abs(golden_data[i+1] - current_data[i+1])
        db = abs(golden_data[i+2] - current_data[i+2])
        
        if dr > tolerance or dg > tolerance or db > tolerance:
            diff_count += 1
    
    total_pixels = (gw * gh)
    diff_percent = (diff_count / total_pixels) * 100 if total_pixels > 0 else 0
    
    if diff_count == 0:
        print("MATCH")
        sys.exit(0)
    else:
        print(f"DIFF: {diff_count} pixels ({diff_percent:.2f}%)")
        sys.exit(1)

except Exception as e:
    print(f"ERROR: {e}")
    sys.exit(2)
EOF
}

# Function to run regression test for a single fixture
test_fixture() {
    local fixture_path="$1"
    local fixture_name=$(basename "$fixture_path" .html)
    local golden_path="$GOLDENS_DIR/${fixture_name}.ppm"
    local current_path="$RESULTS_DIR/${fixture_name}_current.ppm"
    
    echo "Testing: $fixture_name"
    
    # Check if golden exists
    if [ ! -f "$golden_path" ]; then
        echo "  SKIP: No golden image found"
        return 2
    fi
    
    # Capture current rendering
    "$SMOKE_BIN" \
        --html-file "$fixture_path" \
        --duration-ms 1000 \
        --dump-frame "$current_path" \
        2>/dev/null || {
            echo "  FAIL: Capture failed"
            return 1
        }
    
    if [ ! -f "$current_path" ]; then
        echo "  FAIL: No output generated"
        return 1
    fi
    
    # Compare images
    result=$(compare_images "$golden_path" "$current_path")
    
    if [ "$result" = "MATCH" ]; then
        echo "  PASS: Images match"
        rm -f "$current_path"  # Clean up matching images
        return 0
    else
        echo "  FAIL: $result"
        # Keep the current image for review
        cp "$golden_path" "$RESULTS_DIR/${fixture_name}_golden.ppm"
        return 1
    fi
}

echo "=============================="
echo "Visual Regression Tests"
echo "=============================="
echo ""

passed=0
failed=0
skipped=0

# If a specific fixture is requested
if [ -n "$1" ]; then
    fixture_path="$FIXTURES_DIR/$1"
    if [ ! -f "$fixture_path" ]; then
        fixture_path="$FIXTURES_DIR/$1.html"
    fi
    
    if [ -f "$fixture_path" ]; then
        if test_fixture "$fixture_path"; then
            ((passed++))
        else
            ret=$?
            if [ $ret -eq 2 ]; then
                ((skipped++))
            else
                ((failed++))
            fi
        fi
    else
        echo "Fixture not found: $1"
        exit 1
    fi
else
    # Test all fixtures
    for fixture in "$FIXTURES_DIR"/*.html; do
        if [ -f "$fixture" ]; then
            if test_fixture "$fixture"; then
                ((passed++))
            else
                ret=$?
                if [ $ret -eq 2 ]; then
                    ((skipped++))
                else
                    ((failed++))
                fi
            fi
            echo ""
        fi
    done
fi

echo "=============================="
echo "Results: $passed passed, $failed failed, $skipped skipped"
echo "Artifacts: $RESULTS_DIR"
echo "=============================="

if [ $failed -gt 0 ]; then
    exit 1
fi

exit 0

