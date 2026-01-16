#!/bin/bash
# static_suite_capture.sh - Capture RustKit renders for static web suite
#
# This script runs each static web suite test case through RustKit
# and captures the rendered output for validation.
#
# Note: Currently uses direct file:// loading. HTTP server mode TBD.
#
# Usage: ./scripts/static_suite_capture.sh [output_dir]

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SUITE_DIR="$PROJECT_DIR/static-web-suite"
MANIFEST="$SUITE_DIR/manifest.json"
OUTPUT_DIR="${1:-$PROJECT_DIR/static-suite-captures}"
SMOKE_BIN="$PROJECT_DIR/target/release/hiwave-smoke"

echo "Static Web Suite Capture"
echo "========================"
echo ""

# Build if needed
if [ ! -f "$SMOKE_BIN" ]; then
    echo "Building hiwave-smoke..."
    cd "$PROJECT_DIR"
    cargo build -p hiwave-smoke --release
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Check if jq is available
if ! command -v jq &> /dev/null; then
    echo "Error: jq is required but not installed."
    echo "Install with: brew install jq"
    exit 1
fi

# Read test cases from manifest
CASES=$(jq -r '.cases[] | "\(.id):\(.path):\(.viewport.width):\(.viewport.height)"' "$MANIFEST")

TOTAL=0
PASSED=0
FAILED=0

for case_spec in $CASES; do
    IFS=':' read -r case_id case_path width height <<< "$case_spec"
    TOTAL=$((TOTAL + 1))
    
    HTML_PATH="$SUITE_DIR/$case_path"
    OUTPUT_FILE="$OUTPUT_DIR/${case_id}.ppm"
    PERF_FILE="$OUTPUT_DIR/${case_id}.perf.json"
    
    echo "[$TOTAL] Capturing: $case_id ($width x $height)"
    
    if [ ! -f "$HTML_PATH" ]; then
        echo "  SKIP: File not found: $HTML_PATH"
        FAILED=$((FAILED + 1))
        continue
    fi
    
    if "$SMOKE_BIN" \
        --html-file "$HTML_PATH" \
        --width "$width" \
        --height "$height" \
        --duration-ms 1000 \
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
done

echo ""
echo "Static Web Suite Capture Complete"
echo "=================================="
echo "Total:  $TOTAL"
echo "Passed: $PASSED"
echo "Failed: $FAILED"

# Generate summary JSON
python3 << SUMMARY_SCRIPT
import json
import os
from datetime import datetime

output_dir = "$OUTPUT_DIR"
suite_dir = "$SUITE_DIR"
manifest_file = "$MANIFEST"

# Load manifest
with open(manifest_file) as f:
    manifest = json.load(f)

summary = {
    "timestamp": datetime.now().isoformat(),
    "git_sha": os.popen("git rev-parse HEAD").read().strip(),
    "renderer": "rustkit",
    "suite": "static-web-suite",
    "total": len(manifest["cases"]),
    "passed": $PASSED,
    "failed": $FAILED,
    "captures": []
}

for case in manifest["cases"]:
    ppm_file = os.path.join(output_dir, f"{case['id']}.ppm")
    perf_file = os.path.join(output_dir, f"{case['id']}.perf.json")
    
    perf = {}
    if os.path.exists(perf_file):
        with open(perf_file) as f:
            data = json.load(f)
            perf = data.get("perf") or data.get("timings", {})
    
    summary["captures"].append({
        "case_id": case["id"],
        "name": case["name"],
        "source_file": case["path"],
        "viewport": case["viewport"],
        "frame": f"{case['id']}.ppm" if os.path.exists(ppm_file) else None,
        "status": "ok" if os.path.exists(ppm_file) else "fail",
        "perf": perf
    })

with open(os.path.join(output_dir, "summary.json"), "w") as f:
    json.dump(summary, f, indent=2)

print(f"Summary written to {output_dir}/summary.json")
SUMMARY_SCRIPT

if [ $FAILED -gt 0 ]; then
    exit 1
fi

