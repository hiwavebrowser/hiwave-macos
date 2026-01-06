#!/bin/bash
# websuite_capture.sh - Capture frames for all websuite cases
#
# Usage: ./scripts/websuite_capture.sh [output_dir]
#
# This script runs hiwave-smoke for each case in the websuite manifest
# and captures frames for visual regression testing.

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
MANIFEST="$PROJECT_DIR/websuite/manifest.json"
OUTPUT_DIR="${1:-$PROJECT_DIR/websuite/captures}"
SMOKE_BIN="$PROJECT_DIR/target/release/hiwave-smoke"

# Check if manifest exists
if [ ! -f "$MANIFEST" ]; then
    echo "Error: Manifest not found at $MANIFEST"
    exit 1
fi

# Build hiwave-smoke if needed
if [ ! -f "$SMOKE_BIN" ]; then
    echo "Building hiwave-smoke..."
    cd "$PROJECT_DIR"
    cargo build -p hiwave-smoke --release
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Parse manifest and run each case
echo "WebSuite Capture - Starting..."
echo "Output directory: $OUTPUT_DIR"
echo ""

# Use Python to parse JSON manifest
CASES=$(python3 -c "
import json
import sys

with open('$MANIFEST') as f:
    manifest = json.load(f)

for case in manifest['cases']:
    viewport = case.get('viewport', manifest.get('viewport', {'width': 800, 'height': 600}))
    print(f\"{case['id']}|{case['path']}|{viewport['width']}|{viewport['height']}\")
")

TOTAL=0
PASSED=0
FAILED=0

while IFS='|' read -r case_id case_path width height; do
    TOTAL=$((TOTAL + 1))
    echo "[$TOTAL] Capturing: $case_id ($width x $height)"
    
    HTML_FILE="$PROJECT_DIR/websuite/$case_path"
    OUTPUT_FILE="$OUTPUT_DIR/${case_id}.ppm"
    PERF_FILE="$OUTPUT_DIR/${case_id}.perf.json"
    
    if [ ! -f "$HTML_FILE" ]; then
        echo "  SKIP: HTML file not found: $HTML_FILE"
        FAILED=$((FAILED + 1))
        continue
    fi
    
    # Run hiwave-smoke with frame capture
    if "$SMOKE_BIN" \
        --html-file "$HTML_FILE" \
        --width "$width" \
        --height "$height" \
        --duration-ms 500 \
        --dump-frame "$OUTPUT_FILE" \
        --perf-output "$PERF_FILE" \
        2>/dev/null; then
        
        if [ -f "$OUTPUT_FILE" ]; then
            echo "  OK: Frame captured"
            PASSED=$((PASSED + 1))
        else
            echo "  FAIL: Frame not generated"
            FAILED=$((FAILED + 1))
        fi
    else
        echo "  FAIL: hiwave-smoke exited with error"
        FAILED=$((FAILED + 1))
    fi
done <<< "$CASES"

echo ""
echo "WebSuite Capture Complete"
echo "========================="
echo "Total:  $TOTAL"
echo "Passed: $PASSED"
echo "Failed: $FAILED"

# Generate summary JSON
python3 -c "
import json
import os
from datetime import datetime

summary = {
    'timestamp': datetime.now().isoformat(),
    'total': $TOTAL,
    'passed': $PASSED,
    'failed': $FAILED,
    'captures': []
}

for f in os.listdir('$OUTPUT_DIR'):
    if f.endswith('.ppm'):
        case_id = f.replace('.ppm', '')
        perf_file = os.path.join('$OUTPUT_DIR', case_id + '.perf.json')
        perf = {}
        if os.path.exists(perf_file):
            with open(perf_file) as pf:
                perf = json.load(pf)
        summary['captures'].append({
            'case_id': case_id,
            'frame': f,
            'perf': perf.get('perf', {})
        })

with open(os.path.join('$OUTPUT_DIR', 'summary.json'), 'w') as f:
    json.dump(summary, f, indent=2)

print('Summary written to $OUTPUT_DIR/summary.json')
"

if [ $FAILED -gt 0 ]; then
    exit 1
fi

