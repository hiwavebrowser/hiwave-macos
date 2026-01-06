#!/bin/bash
# parity_gate.sh - Unified pixel parity gate for built-ins + websuite
#
# This script:
# 1. Captures RustKit frames for all target pages
# 2. Captures Chromium baselines
# 3. Extracts oracle data (computed styles + DOMRects)
# 4. Compares and generates failure packets
# 5. Reports pass/fail with detailed diagnostics
#
# Usage: ./scripts/parity_gate.sh [--regenerate-baseline] [--verbose]

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BASELINE_TOOL="$PROJECT_DIR/tools/websuite-baseline"
OUTPUT_DIR="$PROJECT_DIR/parity-results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RUN_DIR="$OUTPUT_DIR/$TIMESTAMP"

REGENERATE_BASELINE=false
VERBOSE=false

for arg in "$@"; do
    case $arg in
        --regenerate-baseline)
            REGENERATE_BASELINE=true
            ;;
        --verbose)
            VERBOSE=true
            ;;
    esac
done

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║           PIXEL PARITY GATE (Papa 1 Protocol)                ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "Timestamp: $TIMESTAMP"
echo "Output: $RUN_DIR"
echo ""

# Create output directories
mkdir -p "$RUN_DIR/rustkit"
mkdir -p "$RUN_DIR/chromium"
mkdir -p "$RUN_DIR/oracle"
mkdir -p "$RUN_DIR/diffs"
mkdir -p "$RUN_DIR/failure-packets"

# ============================================================================
# Step 1: Build RustKit smoke harness
# ============================================================================
echo "┌─────────────────────────────────────────────────────────────┐"
echo "│ Step 1: Build RustKit                                       │"
echo "└─────────────────────────────────────────────────────────────┘"

SMOKE_BIN="$PROJECT_DIR/target/release/hiwave-smoke"
if [ ! -f "$SMOKE_BIN" ]; then
    echo "Building hiwave-smoke..."
    cd "$PROJECT_DIR"
    cargo build -p hiwave-smoke --release
fi
echo "OK: hiwave-smoke ready"
echo ""

# ============================================================================
# Step 2: Define target pages
# ============================================================================
echo "┌─────────────────────────────────────────────────────────────┐"
echo "│ Step 2: Define target pages                                 │"
echo "└─────────────────────────────────────────────────────────────┘"

# Built-in pages
BUILTIN_PAGES=(
    "new_tab:$PROJECT_DIR/crates/hiwave-app/src/ui/new_tab.html:1280:800"
    "about:$PROJECT_DIR/crates/hiwave-app/src/ui/about.html:1280:800"
    "settings:$PROJECT_DIR/crates/hiwave-app/src/ui/settings.html:1280:800"
)

# Websuite pages
WEBSUITE_PAGES=()
if [ -f "$PROJECT_DIR/websuite/manifest.json" ]; then
    while IFS= read -r line; do
        WEBSUITE_PAGES+=("$line")
    done < <(python3 -c "
import json
with open('$PROJECT_DIR/websuite/manifest.json') as f:
    m = json.load(f)
for c in m.get('cases', []):
    # Path is relative to websuite/cases/
    print(f\"{c['id']}:$PROJECT_DIR/websuite/cases/{c['id']}/index.html:{c['viewport']['width']}:{c['viewport']['height']}\")
")
fi

ALL_PAGES=("${BUILTIN_PAGES[@]}" "${WEBSUITE_PAGES[@]}")
echo "Target pages: ${#ALL_PAGES[@]}"
for page in "${ALL_PAGES[@]}"; do
    IFS=':' read -r id path w h <<< "$page"
    echo "  - $id (${w}x${h})"
done
echo ""

# ============================================================================
# Step 3: Capture RustKit frames
# ============================================================================
echo "┌─────────────────────────────────────────────────────────────┐"
echo "│ Step 3: Capture RustKit frames                              │"
echo "└─────────────────────────────────────────────────────────────┘"

RUSTKIT_PASSED=0
RUSTKIT_FAILED=0

for page in "${ALL_PAGES[@]}"; do
    IFS=':' read -r id html_path width height <<< "$page"
    
    if [ ! -f "$html_path" ]; then
        echo "  SKIP: $id (file not found)"
        RUSTKIT_FAILED=$((RUSTKIT_FAILED + 1))
        continue
    fi
    
    OUTPUT_PPM="$RUN_DIR/rustkit/${id}.ppm"
    PERF_JSON="$RUN_DIR/rustkit/${id}.perf.json"
    
    echo -n "  Capturing $id... "
    
    if "$SMOKE_BIN" \
        --html-file "$html_path" \
        --width "$width" \
        --height "$height" \
        --duration-ms 500 \
        --dump-frame "$OUTPUT_PPM" \
        --perf-output "$PERF_JSON" \
        2>/dev/null; then
        
        if [ -f "$OUTPUT_PPM" ]; then
            echo "OK"
            RUSTKIT_PASSED=$((RUSTKIT_PASSED + 1))
        else
            echo "FAIL (no frame)"
            RUSTKIT_FAILED=$((RUSTKIT_FAILED + 1))
        fi
    else
        echo "FAIL (crash)"
        RUSTKIT_FAILED=$((RUSTKIT_FAILED + 1))
    fi
done

echo ""
echo "RustKit: $RUSTKIT_PASSED passed, $RUSTKIT_FAILED failed"
echo ""

# ============================================================================
# Step 4: Capture Chromium baselines + oracle data
# ============================================================================
echo "┌─────────────────────────────────────────────────────────────┐"
echo "│ Step 4: Capture Chromium baselines + oracle                 │"
echo "└─────────────────────────────────────────────────────────────┘"

cd "$BASELINE_TOOL"

# Ensure npm is available
NPM_BIN=$(which npm 2>/dev/null || echo "/opt/homebrew/bin/npm")
if [ ! -x "$NPM_BIN" ]; then
    NPM_BIN="/usr/local/bin/npm"
fi

if [ ! -x "$NPM_BIN" ]; then
    echo "WARNING: npm not found, skipping Chromium baseline capture"
    echo "Install Node.js to enable Chromium baseline capture"
    CHROMIUM_PASSED=0
    CHROMIUM_FAILED=${#ALL_PAGES[@]}
else
    # Install dependencies if needed
    if [ ! -d "node_modules" ]; then
        echo "Installing Playwright..."
        "$NPM_BIN" install
        "$(dirname "$NPM_BIN")/npx" playwright install chromium
    fi
fi

CHROMIUM_PASSED=0
CHROMIUM_FAILED=0

if [ -x "$NPM_BIN" ]; then
    for page in "${ALL_PAGES[@]}"; do
        IFS=':' read -r id html_path width height <<< "$page"
        
        if [ ! -f "$html_path" ]; then
            continue
        fi
        
        echo -n "  Capturing $id (Chromium + oracle)... "
        
        # Capture screenshot and oracle
        if node extract_oracle.js "$html_path" "$RUN_DIR/oracle" 2>/dev/null; then
            # Move screenshot to chromium dir
            BASE_NAME=$(basename "$html_path" .html)
            if [ -f "$RUN_DIR/oracle/${BASE_NAME}.chromium.png" ]; then
                mv "$RUN_DIR/oracle/${BASE_NAME}.chromium.png" "$RUN_DIR/chromium/${id}.png"
            fi
            if [ -f "$RUN_DIR/oracle/${BASE_NAME}.oracle.json" ]; then
                mv "$RUN_DIR/oracle/${BASE_NAME}.oracle.json" "$RUN_DIR/oracle/${id}.oracle.json"
            fi
            echo "OK"
            CHROMIUM_PASSED=$((CHROMIUM_PASSED + 1))
        else
            echo "FAIL"
            CHROMIUM_FAILED=$((CHROMIUM_FAILED + 1))
        fi
    done
fi

echo ""
echo "Chromium: $CHROMIUM_PASSED passed, $CHROMIUM_FAILED failed"
echo ""

# ============================================================================
# Step 5: Compare and generate failure packets
# ============================================================================
echo "┌─────────────────────────────────────────────────────────────┐"
echo "│ Step 5: Compare frames + generate failure packets           │"
echo "└─────────────────────────────────────────────────────────────┘"

cd "$BASELINE_TOOL"

COMPARE_PASSED=0
COMPARE_FAILED=0

for page in "${ALL_PAGES[@]}"; do
    IFS=':' read -r id html_path width height <<< "$page"
    
    RUSTKIT_PPM="$RUN_DIR/rustkit/${id}.ppm"
    CHROMIUM_PNG="$RUN_DIR/chromium/${id}.png"
    
    if [ ! -f "$RUSTKIT_PPM" ] || [ ! -f "$CHROMIUM_PNG" ]; then
        echo "  SKIP: $id (missing frames)"
        continue
    fi
    
    echo -n "  Comparing $id... "
    
    if node compare_pixels.js "$RUSTKIT_PPM" "$CHROMIUM_PNG" "$RUN_DIR/diffs" 2>/dev/null; then
        echo "PASS"
        COMPARE_PASSED=$((COMPARE_PASSED + 1))
    else
        echo "DIFF"
        COMPARE_FAILED=$((COMPARE_FAILED + 1))
        
        # Generate failure packet
        python3 "$PROJECT_DIR/scripts/generate_failure_packet.py" "$id" "$RUN_DIR" 2>/dev/null || true
    fi
done

echo ""
echo "Comparison: $COMPARE_PASSED passed, $COMPARE_FAILED failed"
echo ""

# Generate consolidated summary
cd "$PROJECT_DIR"
python3 << SUMMARY_SCRIPT
import json
import os
from datetime import datetime
from pathlib import Path

run_dir = Path("$RUN_DIR")
diffs_dir = run_dir / "diffs"
packets_dir = run_dir / "failure-packets"

results = {
    'timestamp': datetime.now().isoformat(),
    'policy': {
        'aa_tolerance': 5,
        'max_diff_percent': 0.0
    },
    'summary': {
        'total': 0,
        'passed': 0,
        'failed': 0,
        'true_diff_pixels': 0,
        'tolerated_pixels': 0,
        'total_pixels': 0
    },
    'cases': []
}

# Load all comparison reports
for comp_file in diffs_dir.glob("*.comparison.json"):
    with open(comp_file) as f:
        comp = json.load(f)
    
    results['summary']['total'] += 1
    
    comp_data = comp.get('comparison', {})
    passed = comp_data.get('passed', False)
    
    if passed:
        results['summary']['passed'] += 1
    else:
        results['summary']['failed'] += 1
    
    results['summary']['true_diff_pixels'] += comp_data.get('true_diff_pixels', 0)
    results['summary']['tolerated_pixels'] += comp_data.get('tolerated_diff_pixels', 0)
    results['summary']['total_pixels'] += comp_data.get('total_pixels', 0)
    
    results['cases'].append({
        'id': comp.get('case_id'),
        'status': 'pass' if passed else 'fail',
        'true_diff_pixels': comp_data.get('true_diff_pixels', 0),
        'true_diff_percent': comp_data.get('true_diff_percent', 0),
        'tolerated_pixels': comp_data.get('tolerated_diff_pixels', 0),
        'failure_category': comp.get('failure_category')
    })

# Calculate overall metrics
total_px = results['summary']['total_pixels']
if total_px > 0:
    results['summary']['true_diff_rate'] = results['summary']['true_diff_pixels'] / total_px
    results['summary']['web_score'] = 1.0 - results['summary']['true_diff_rate']
else:
    results['summary']['true_diff_rate'] = 0
    results['summary']['web_score'] = 1.0

# Write summary
summary_path = run_dir / 'parity_summary.json'
with open(summary_path, 'w') as f:
    json.dump(results, f, indent=2)

print(f"Summary written to: {summary_path}")
print(f"\nOverall Results:")
print(f"  Total cases: {results['summary']['total']}")
print(f"  Passed: {results['summary']['passed']}")
print(f"  Failed: {results['summary']['failed']}")
print(f"  True diff pixels: {results['summary']['true_diff_pixels']}")
print(f"  Tolerated pixels: {results['summary']['tolerated_pixels']}")
print(f"  Web Score: {results['summary']['web_score']:.4f}")
SUMMARY_SCRIPT

echo ""

# ============================================================================
# Step 6: Generate final report
# ============================================================================
echo "┌─────────────────────────────────────────────────────────────┐"
echo "│ Step 6: Final Report                                        │"
echo "└─────────────────────────────────────────────────────────────┘"

# Create symlink to latest
rm -f "$OUTPUT_DIR/latest"
ln -s "$RUN_DIR" "$OUTPUT_DIR/latest"

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║                    PARITY GATE COMPLETE                      ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "Results: $RUN_DIR"
echo "Latest:  $OUTPUT_DIR/latest"
echo ""
echo "Artifacts:"
echo "  - RustKit frames:    $RUN_DIR/rustkit/"
echo "  - Chromium frames:   $RUN_DIR/chromium/"
echo "  - Oracle data:       $RUN_DIR/oracle/"
echo "  - Diff images:       $RUN_DIR/diffs/"
echo "  - Failure packets:   $RUN_DIR/failure-packets/"
echo "  - Summary:           $RUN_DIR/parity_summary.json"
echo ""

# Return exit code based on results
if [ $RUSTKIT_FAILED -gt 0 ] || [ $CHROMIUM_FAILED -gt 0 ]; then
    echo "STATUS: FAIL (capture errors)"
    exit 1
fi

if [ $COMPARE_FAILED -gt 0 ]; then
    echo "STATUS: FAIL ($COMPARE_FAILED cases have pixel differences)"
    echo ""
    echo "To investigate failures, check:"
    echo "  - Diff images: $RUN_DIR/diffs/"
    echo "  - Failure packets: $RUN_DIR/failure-packets/"
    echo "  - Oracle data: $RUN_DIR/oracle/"
    exit 1
fi

echo "STATUS: PASS (all cases match within AA tolerance)"
exit 0

