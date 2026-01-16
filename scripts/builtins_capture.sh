#!/bin/bash
# builtins_capture.sh - Capture built-in pages with RustKit engine
#
# This script captures deterministic frames for all built-in HiWave pages
# using the RustKit engine at standardized viewports.
#
# Usage: ./scripts/builtins_capture.sh [output_dir]

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
UI_DIR="$PROJECT_DIR/crates/hiwave-app/src/ui"
OUTPUT_DIR="${1:-$PROJECT_DIR/builtins-captures}"
SMOKE_BIN="$PROJECT_DIR/target/release/hiwave-smoke"

echo "Built-in Pages Capture (RustKit)"
echo "================================="
echo ""

# Build if needed
if [ ! -f "$SMOKE_BIN" ]; then
    echo "Building hiwave-smoke..."
    cd "$PROJECT_DIR"
    cargo build -p hiwave-smoke --release
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Define built-in pages to capture
PAGES=(
    "new_tab:new_tab.html:1280:800"
    "about:about.html:1280:800"
    "settings:settings.html:1280:800"
    "chrome:chrome.html:1280:72"
    "shelf:shelf.html:1280:120"
)

TOTAL=0
PASSED=0
FAILED=0

for page_spec in "${PAGES[@]}"; do
    IFS=':' read -r page_id page_file width height <<< "$page_spec"
    TOTAL=$((TOTAL + 1))
    
    HTML_PATH="$UI_DIR/$page_file"
    OUTPUT_FILE="$OUTPUT_DIR/${page_id}.ppm"
    PERF_FILE="$OUTPUT_DIR/${page_id}.perf.json"
    
    echo "[$TOTAL/${#PAGES[@]}] Capturing: $page_id ($width x $height)"
    
    if [ ! -f "$HTML_PATH" ]; then
        echo "  SKIP: File not found: $HTML_PATH"
        FAILED=$((FAILED + 1))
        continue
    fi
    
    if "$SMOKE_BIN" \
        --html-file "$HTML_PATH" \
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
done

echo ""
echo "Built-in Pages Capture Complete"
echo "================================"
echo "Total:  $TOTAL"
echo "Passed: $PASSED"
echo "Failed: $FAILED"

# Generate summary JSON
python3 << SUMMARY_SCRIPT
import json
import os
from datetime import datetime

output_dir = "$OUTPUT_DIR"
pages = [
    {"id": "new_tab", "file": "new_tab.html", "viewport": {"width": 1280, "height": 800}},
    {"id": "about", "file": "about.html", "viewport": {"width": 1280, "height": 800}},
    {"id": "settings", "file": "settings.html", "viewport": {"width": 1280, "height": 800}},
    {"id": "chrome", "file": "chrome.html", "viewport": {"width": 1280, "height": 72}},
    {"id": "shelf", "file": "shelf.html", "viewport": {"width": 1280, "height": 120}},
]

summary = {
    "timestamp": datetime.now().isoformat(),
    "git_sha": os.popen("git rev-parse HEAD").read().strip(),
    "renderer": "rustkit",
    "dpr": 2.0,
    "total": len(pages),
    "passed": $PASSED,
    "failed": $FAILED,
    "captures": []
}

for page in pages:
    ppm_file = os.path.join(output_dir, f"{page['id']}.ppm")
    perf_file = os.path.join(output_dir, f"{page['id']}.perf.json")
    
    perf = {}
    if os.path.exists(perf_file):
        with open(perf_file) as f:
            perf = json.load(f).get("perf", {})
    
    summary["captures"].append({
        "page_id": page["id"],
        "source_file": page["file"],
        "viewport": page["viewport"],
        "frame": f"{page['id']}.ppm" if os.path.exists(ppm_file) else None,
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

