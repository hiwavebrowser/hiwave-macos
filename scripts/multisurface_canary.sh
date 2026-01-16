#!/bin/bash
# multisurface_canary.sh - Test multisurface RustKit rendering
#
# This script captures frames from both chrome and content RustKit views
# to verify multisurface compositing works correctly.
#
# Usage: ./scripts/multisurface_canary.sh [output_dir]

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_DIR="${1:-$PROJECT_DIR/multisurface-captures}"
SMOKE_BIN="$PROJECT_DIR/target/release/hiwave-smoke"

echo "Multisurface Canary Test"
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

# Capture chrome view
echo "[1/2] Capturing Chrome view..."
"$SMOKE_BIN" \
    --html-file "$PROJECT_DIR/fixtures/multisurface_chrome.html" \
    --width 1100 \
    --height 72 \
    --duration-ms 500 \
    --dump-frame "$OUTPUT_DIR/chrome.ppm" \
    --perf-output "$OUTPUT_DIR/chrome.perf.json" \
    2>/dev/null

if [ -f "$OUTPUT_DIR/chrome.ppm" ]; then
    echo "  OK: Chrome frame captured"
    CHROME_OK=true
else
    echo "  FAIL: Chrome frame not generated"
    CHROME_OK=false
fi

# Capture content view
echo "[2/2] Capturing Content view..."
"$SMOKE_BIN" \
    --html-file "$PROJECT_DIR/fixtures/multisurface_content.html" \
    --width 1100 \
    --height 600 \
    --duration-ms 500 \
    --dump-frame "$OUTPUT_DIR/content.ppm" \
    --perf-output "$OUTPUT_DIR/content.perf.json" \
    2>/dev/null

if [ -f "$OUTPUT_DIR/content.ppm" ]; then
    echo "  OK: Content frame captured"
    CONTENT_OK=true
else
    echo "  FAIL: Content frame not generated"
    CONTENT_OK=false
fi

# Generate summary
echo ""
echo "Multisurface Canary Complete"
echo "============================"

python3 << SUMMARY_SCRIPT
import json
import os
from datetime import datetime

output_dir = "$OUTPUT_DIR"
chrome_ok = "$CHROME_OK" == "true"
content_ok = "$CONTENT_OK" == "true"

# Load perf data
chrome_perf = {}
content_perf = {}

chrome_perf_file = os.path.join(output_dir, "chrome.perf.json")
content_perf_file = os.path.join(output_dir, "content.perf.json")

if os.path.exists(chrome_perf_file):
    with open(chrome_perf_file) as f:
        chrome_perf = json.load(f).get("perf", {})

if os.path.exists(content_perf_file):
    with open(content_perf_file) as f:
        content_perf = json.load(f).get("perf", {})

summary = {
    "timestamp": datetime.now().isoformat(),
    "status": "pass" if (chrome_ok and content_ok) else "fail",
    "surfaces": {
        "chrome": {
            "status": "ok" if chrome_ok else "fail",
            "dimensions": {"width": 1100, "height": 72},
            "frame": "chrome.ppm" if chrome_ok else None,
            "perf": chrome_perf
        },
        "content": {
            "status": "ok" if content_ok else "fail",
            "dimensions": {"width": 1100, "height": 600},
            "frame": "content.ppm" if content_ok else None,
            "perf": content_perf
        }
    },
    "notes": "Separate captures - compositor integration pending"
}

with open(os.path.join(output_dir, "summary.json"), "w") as f:
    json.dump(summary, f, indent=2)

print(f"Chrome:  {'PASS' if chrome_ok else 'FAIL'}")
print(f"Content: {'PASS' if content_ok else 'FAIL'}")
print(f"Overall: {summary['status'].upper()}")
print(f"\nResults: {output_dir}")
SUMMARY_SCRIPT

# Exit with appropriate code
if [ "$CHROME_OK" = true ] && [ "$CONTENT_OK" = true ]; then
    exit 0
else
    exit 1
fi

