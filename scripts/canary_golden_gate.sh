#!/bin/bash
# canary_golden_gate.sh - Golden image diff gate for canary runs
#
# Produces a JSON diff report comparing current render against golden.
# Exit code 0 = pass (diff within tolerance), 1 = fail (regression)
#
# Usage:
#   ./scripts/canary_golden_gate.sh --fixture typography --output /tmp/diff_report.json
#   ./scripts/canary_golden_gate.sh --all --output /tmp/all_diffs.json

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURES_DIR="$PROJECT_ROOT/fixtures"
GOLDENS_DIR="$PROJECT_ROOT/goldens"
SMOKE_BIN="$PROJECT_ROOT/target/release/hiwave-smoke"

# Default capture size for deterministic comparison
CAPTURE_WIDTH=800
CAPTURE_HEIGHT=600
CAPTURE_DURATION=1000

# Tolerance for pixel comparison (0-255 per channel)
TOLERANCE=5

# Parse arguments
FIXTURE=""
OUTPUT_FILE=""
RUN_ALL=false
TEMP_DIR=$(mktemp -d)

while [[ $# -gt 0 ]]; do
    case $1 in
        --fixture)
            FIXTURE="$2"
            shift 2
            ;;
        --all)
            RUN_ALL=true
            shift
            ;;
        --output)
            OUTPUT_FILE="$2"
            shift 2
            ;;
        --tolerance)
            TOLERANCE="$2"
            shift 2
            ;;
        --width)
            CAPTURE_WIDTH="$2"
            shift 2
            ;;
        --height)
            CAPTURE_HEIGHT="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

# Ensure smoke binary exists
if [ ! -f "$SMOKE_BIN" ]; then
    echo "Building hiwave-smoke..." >&2
    cd "$PROJECT_ROOT"
    cargo build -p hiwave-smoke --release 2>/dev/null || {
        SMOKE_BIN="$PROJECT_ROOT/target/debug/hiwave-smoke"
        cargo build -p hiwave-smoke 2>/dev/null
    }
fi

# Python function to compare PPM images and output JSON
compare_and_report() {
    local fixture_name="$1"
    local golden_path="$2"
    local current_path="$3"
    
    python3 << EOF
import json
import sys
import os

def read_ppm(path):
    try:
        with open(path, 'rb') as f:
            magic = f.readline().decode().strip()
            if magic != 'P6':
                return None, 0, 0
            line = f.readline().decode()
            while line.startswith('#'):
                line = f.readline().decode()
            dims = line.strip().split()
            width, height = int(dims[0]), int(dims[1])
            max_val = int(f.readline().decode().strip())
            data = f.read()
            return data, width, height
    except Exception as e:
        return None, 0, 0

fixture_name = "$fixture_name"
golden_path = "$golden_path"
current_path = "$current_path"
tolerance = $TOLERANCE

result = {
    "fixture": fixture_name,
    "golden_path": golden_path,
    "current_path": current_path,
    "status": "unknown",
    "diff_pixels": 0,
    "total_pixels": 0,
    "diff_percent": 0.0,
    "tolerance": tolerance,
    "dimensions": {"width": 0, "height": 0},
    "error": None
}

if not os.path.exists(golden_path):
    result["status"] = "no_golden"
    result["error"] = f"Golden image not found: {golden_path}"
    print(json.dumps(result))
    sys.exit(0)

if not os.path.exists(current_path):
    result["status"] = "capture_failed"
    result["error"] = f"Current capture not found: {current_path}"
    print(json.dumps(result))
    sys.exit(1)

golden_data, gw, gh = read_ppm(golden_path)
current_data, cw, ch = read_ppm(current_path)

if golden_data is None:
    result["status"] = "invalid_golden"
    result["error"] = "Failed to read golden PPM"
    print(json.dumps(result))
    sys.exit(1)

if current_data is None:
    result["status"] = "invalid_current"
    result["error"] = "Failed to read current PPM"
    print(json.dumps(result))
    sys.exit(1)

if (gw, gh) != (cw, ch):
    result["status"] = "size_mismatch"
    result["error"] = f"Size mismatch: golden={gw}x{gh}, current={cw}x{ch}"
    result["dimensions"] = {"golden": {"width": gw, "height": gh}, "current": {"width": cw, "height": ch}}
    print(json.dumps(result))
    sys.exit(1)

result["dimensions"] = {"width": gw, "height": gh}
result["total_pixels"] = gw * gh

# Compare pixels
diff_count = 0
for i in range(0, min(len(golden_data), len(current_data)), 3):
    if i + 2 >= len(golden_data) or i + 2 >= len(current_data):
        break
    dr = abs(golden_data[i] - current_data[i])
    dg = abs(golden_data[i+1] - current_data[i+1])
    db = abs(golden_data[i+2] - current_data[i+2])
    if dr > tolerance or dg > tolerance or db > tolerance:
        diff_count += 1

result["diff_pixels"] = diff_count
result["diff_percent"] = (diff_count / (gw * gh)) * 100 if gw * gh > 0 else 0

if diff_count == 0:
    result["status"] = "pass"
else:
    result["status"] = "diff"

print(json.dumps(result, indent=2))
sys.exit(0 if diff_count == 0 else 1)
EOF
}

# Run a single fixture test
run_fixture_test() {
    local fixture_name="$1"
    local fixture_path="$FIXTURES_DIR/${fixture_name}.html"
    local golden_path="$GOLDENS_DIR/${fixture_name}.ppm"
    local current_path="$TEMP_DIR/${fixture_name}_current.ppm"
    
    # Capture current frame
    "$SMOKE_BIN" \
        --html-file "$fixture_path" \
        --width "$CAPTURE_WIDTH" \
        --height "$CAPTURE_HEIGHT" \
        --duration-ms "$CAPTURE_DURATION" \
        --dump-frame "$current_path" \
        2>/dev/null || true
    
    # Compare and report
    compare_and_report "$fixture_name" "$golden_path" "$current_path"
}

# Main execution
if [ -n "$FIXTURE" ]; then
    # Single fixture test
    result=$(run_fixture_test "$FIXTURE")
    
    if [ -n "$OUTPUT_FILE" ]; then
        echo "$result" > "$OUTPUT_FILE"
    else
        echo "$result"
    fi
    
    # Check status for exit code - extract from JSON result
    status=$(echo "$result" | grep -o '"status": *"[^"]*"' | head -1 | sed 's/.*"\([^"]*\)"$/\1/')
    [ "$status" = "pass" ] && exit 0 || exit 1

elif [ "$RUN_ALL" = true ]; then
    # Run all fixtures
    results="[]"
    overall_pass=true
    
    for fixture_path in "$FIXTURES_DIR"/*.html; do
        if [ -f "$fixture_path" ]; then
            fixture_name=$(basename "$fixture_path" .html)
            result=$(run_fixture_test "$fixture_name" 2>/dev/null || echo '{"status":"error","fixture":"'$fixture_name'"}')
            
            # Append to results array
            results=$(echo "$results" | python3 -c "
import json, sys
arr = json.load(sys.stdin)
arr.append($result)
print(json.dumps(arr))
")
            
            # Check if this one failed
            status=$(echo "$result" | python3 -c "import json,sys; print(json.load(sys.stdin).get('status','fail'))")
            if [ "$status" != "pass" ] && [ "$status" != "no_golden" ]; then
                overall_pass=false
            fi
        fi
    done
    
    # Create summary report
    summary=$(python3 << EOF
import json
results = $results
passed = sum(1 for r in results if r.get('status') == 'pass')
failed = sum(1 for r in results if r.get('status') in ['diff', 'error', 'capture_failed'])
no_golden = sum(1 for r in results if r.get('status') == 'no_golden')

report = {
    "summary": {
        "total": len(results),
        "passed": passed,
        "failed": failed,
        "no_golden": no_golden,
        "status": "pass" if failed == 0 else "fail"
    },
    "capture_config": {
        "width": $CAPTURE_WIDTH,
        "height": $CAPTURE_HEIGHT,
        "tolerance": $TOLERANCE
    },
    "fixtures": results
}
print(json.dumps(report, indent=2))
EOF
)
    
    if [ -n "$OUTPUT_FILE" ]; then
        echo "$summary" > "$OUTPUT_FILE"
    else
        echo "$summary"
    fi
    
    [ "$overall_pass" = true ] && exit 0 || exit 1
else
    echo "Usage: $0 --fixture <name> [--output <file>]" >&2
    echo "       $0 --all [--output <file>]" >&2
    exit 1
fi

# Cleanup
rm -rf "$TEMP_DIR"

