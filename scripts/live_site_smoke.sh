#!/bin/bash
# live_site_smoke.sh - Run non-gating live site smoke tests
#
# This script tests RustKit rendering against real websites.
# Results are collected but not gating - failures are expected as
# we work toward full web compatibility.
#
# Usage: ./scripts/live_site_smoke.sh [output_dir] [--chromium-only] [--rustkit-only]
#
# Modes:
#   --chromium-only: Only capture Chromium baselines (default until HTTP works)
#   --rustkit-only: Only capture RustKit renders (requires HTTP(S) support)
#   (default): Capture both and compare

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CONFIG="$PROJECT_DIR/websuite/live-sites.json"
OUTPUT_DIR="${1:-$PROJECT_DIR/websuite/live-site-results}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RUN_DIR="$OUTPUT_DIR/$TIMESTAMP"

# Parse arguments
MODE="chromium-only"  # Default to Chromium only until HTTP works
for arg in "$@"; do
    case $arg in
        --chromium-only)
            MODE="chromium-only"
            ;;
        --rustkit-only)
            MODE="rustkit-only"
            ;;
        --both)
            MODE="both"
            ;;
    esac
done

echo "Live Site Smoke Test"
echo "===================="
echo "Mode: $MODE"
echo ""
echo "WARNING: This is non-gating. Failures are expected during development."
echo ""

# Check config exists
if [ ! -f "$CONFIG" ]; then
    echo "Error: Config not found at $CONFIG"
    exit 1
fi

# Create output directories
mkdir -p "$RUN_DIR/chromium"
mkdir -p "$RUN_DIR/rustkit"
mkdir -p "$RUN_DIR/diffs"

# Build if needed (for RustKit HTTP support)
if [ "$MODE" != "chromium-only" ]; then
    if [ ! -f "$PROJECT_DIR/target/release/hiwave" ]; then
        echo "Building HiWave..."
        cd "$PROJECT_DIR"
        cargo build --release -p hiwave-app || true
    fi
fi

BASELINE_TOOL="$PROJECT_DIR/tools/websuite-baseline"

# ============================================================================
# Chromium Capture
# ============================================================================

if [ "$MODE" != "rustkit-only" ]; then
    echo "=== Chromium Capture ==="
    echo ""

    if [ ! -d "$BASELINE_TOOL/node_modules" ]; then
        echo "Installing Playwright..."
        cd "$BASELINE_TOOL"
        npm install
        npx playwright install chromium
    fi

# Create capture script for live sites
cat > "$RUN_DIR/capture_live.js" << 'CAPTURE_SCRIPT'
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');

async function main() {
    const configPath = process.argv[2];
    const outputDir = process.argv[3];
    
    const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
    
    const browser = await chromium.launch({ headless: true });
    const results = [];
    
    for (const site of config.sites) {
        console.log(`Capturing: ${site.name} (${site.url})`);
        
        try {
            const context = await browser.newContext({
                viewport: site.viewport,
                deviceScaleFactor: 2,
            });
            
            const page = await context.newPage();
            
            // Collect console logs
            const consoleLogs = [];
            page.on('console', msg => {
                consoleLogs.push({
                    type: msg.type(),
                    text: msg.text(),
                    time: new Date().toISOString()
                });
            });
            
            // Collect network requests
            const networkLogs = [];
            page.on('request', req => {
                networkLogs.push({
                    url: req.url(),
                    method: req.method(),
                    resourceType: req.resourceType(),
                    time: new Date().toISOString()
                });
            });
            
            const startTime = Date.now();
            
            await page.goto(site.url, {
                waitUntil: 'networkidle',
                timeout: config.output_config.timeout_ms
            });
            
            await page.waitForTimeout(site.wait_ms);
            
            const loadTime = Date.now() - startTime;
            
            // Capture screenshot
            const screenshotPath = path.join(outputDir, `${site.id}.png`);
            await page.screenshot({ path: screenshotPath, fullPage: false });
            
            await context.close();
            
            // Save logs
            fs.writeFileSync(
                path.join(outputDir, `${site.id}.console.json`),
                JSON.stringify(consoleLogs, null, 2)
            );
            fs.writeFileSync(
                path.join(outputDir, `${site.id}.network.json`),
                JSON.stringify(networkLogs, null, 2)
            );
            
            results.push({
                id: site.id,
                name: site.name,
                url: site.url,
                status: 'ok',
                load_time_ms: loadTime,
                console_log_count: consoleLogs.length,
                network_request_count: networkLogs.length
            });
            
            console.log(`  OK: ${loadTime}ms, ${networkLogs.length} requests`);
            
        } catch (error) {
            console.log(`  FAIL: ${error.message}`);
            results.push({
                id: site.id,
                name: site.name,
                url: site.url,
                status: 'error',
                error: error.message
            });
        }
    }
    
    await browser.close();
    
    // Write summary
    fs.writeFileSync(
        path.join(outputDir, 'summary.json'),
        JSON.stringify({
            timestamp: new Date().toISOString(),
            total: config.sites.length,
            passed: results.filter(r => r.status === 'ok').length,
            failed: results.filter(r => r.status !== 'ok').length,
            sites: results
        }, null, 2)
    );
}

main().catch(console.error);
CAPTURE_SCRIPT

# Run Chromium capture
cd "$BASELINE_TOOL"
node "$RUN_DIR/capture_live.js" "$CONFIG" "$RUN_DIR/chromium"

# Copy summary to root with chromium prefix
mv "$RUN_DIR/chromium/summary.json" "$RUN_DIR/chromium_summary.json"

fi # end Chromium capture

# ============================================================================
# RustKit Capture (when HTTP(S) support is available)
# ============================================================================

if [ "$MODE" != "chromium-only" ]; then
    echo ""
    echo "=== RustKit Capture ==="
    echo ""
    
    SMOKE_BIN="$PROJECT_DIR/target/release/hiwave-smoke"
    
    if [ ! -f "$SMOKE_BIN" ]; then
        echo "Building hiwave-smoke..."
        cd "$PROJECT_DIR"
        cargo build -p hiwave-smoke --release
    fi
    
    # Note: RustKit live site capture requires HTTP(S) network support
    # which is not yet fully implemented. For now, we skip this step.
    echo "NOTICE: RustKit live site capture requires HTTP(S) network support."
    echo "        This feature is in development. Skipping RustKit capture."
    echo ""
    
    # Create placeholder summary
    python3 << RUSTKIT_SUMMARY
import json
import os
from datetime import datetime

config_path = "$CONFIG"
output_dir = "$RUN_DIR/rustkit"

with open(config_path) as f:
    config = json.load(f)

summary = {
    "timestamp": datetime.now().isoformat(),
    "renderer": "rustkit",
    "status": "skipped",
    "reason": "HTTP(S) network support not yet implemented",
    "total": len(config.get("sites", [])),
    "passed": 0,
    "failed": 0,
    "skipped": len(config.get("sites", [])),
    "sites": [
        {
            "id": site["id"],
            "name": site["name"],
            "url": site["url"],
            "status": "skipped",
            "reason": "HTTP(S) not implemented"
        }
        for site in config.get("sites", [])
    ]
}

with open(os.path.join(output_dir, "summary.json"), "w") as f:
    json.dump(summary, f, indent=2)

print("RustKit summary created (skipped)")
RUSTKIT_SUMMARY
    
    mv "$RUN_DIR/rustkit/summary.json" "$RUN_DIR/rustkit_summary.json"

fi # end RustKit capture

# ============================================================================
# Comparison (when both captures are available)
# ============================================================================

if [ "$MODE" = "both" ]; then
    echo ""
    echo "=== Comparison ==="
    echo ""
    
    echo "NOTICE: Comparison requires both Chromium and RustKit captures."
    echo "        RustKit capture is currently skipped."
    echo ""
fi

# ============================================================================
# Final Summary
# ============================================================================

echo ""
echo "Live Site Smoke Complete"
echo "========================"
echo "Results: $RUN_DIR"
echo ""

# Print Chromium summary if available
if [ -f "$RUN_DIR/chromium_summary.json" ]; then
    echo "Chromium Results:"
    cat "$RUN_DIR/chromium_summary.json" | python3 -c "
import json, sys
data = json.load(sys.stdin)
print(f\"  Total:  {data['total']}\")
print(f\"  Passed: {data['passed']}\")
print(f\"  Failed: {data['failed']}\")
print()
print('  Sites:')
for site in data['sites']:
    status = '✓' if site['status'] == 'ok' else '✗'
    print(f\"    {status} {site['name']}\")
"
fi

# Print RustKit summary if available
if [ -f "$RUN_DIR/rustkit_summary.json" ]; then
    echo ""
    echo "RustKit Results:"
    cat "$RUN_DIR/rustkit_summary.json" | python3 -c "
import json, sys
data = json.load(sys.stdin)
if data.get('status') == 'skipped':
    print(f\"  Status: Skipped ({data.get('reason', 'unknown')})\")
else:
    print(f\"  Total:  {data['total']}\")
    print(f\"  Passed: {data['passed']}\")
    print(f\"  Failed: {data['failed']}\")
"
fi

