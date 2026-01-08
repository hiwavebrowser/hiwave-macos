#!/bin/bash
# Visual Test Runner - Shows each parity test case in a window
# Usage: ./scripts/visual_test_runner.sh [--duration <ms>] [--case <name>]

set -e

DURATION_MS=5000
SINGLE_CASE=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --duration)
            DURATION_MS="$2"
            shift 2
            ;;
        --case)
            SINGLE_CASE="$2"
            shift 2
            ;;
        --help)
            echo "Usage: $0 [--duration <ms>] [--case <name>]"
            echo ""
            echo "Options:"
            echo "  --duration <ms>  How long to show each page (default: 5000)"
            echo "  --case <name>    Run only a single case"
            echo ""
            echo "Available cases:"
            echo "  Built-ins: new_tab, about, settings, chrome_rustkit, shelf"
            echo "  Websuite: article-typography, card-grid, css-selectors,"
            echo "            flex-positioning, form-elements, gradient-backgrounds,"
            echo "            image-gallery, sticky-scroll"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

run_case() {
    local name="$1"
    local html="$2"
    local width="$3"
    local height="$4"
    
    echo ""
    echo "--- $name ---"
    echo "  File: $html"
    echo "  Size: ${width}x${height}"
    echo "  Opening window for ${DURATION_MS}ms..."
    
    if cargo run --release -p hiwave-smoke -- \
        --html-file "$html" \
        --width "$width" \
        --height "$height" \
        --duration-ms "$DURATION_MS" 2>&1; then
        echo "  ✓ Done"
        return 0
    else
        echo "  ✗ Error occurred"
        return 1
    fi
}

echo "=============================================="
echo "Visual Test Runner"
echo "Duration per case: ${DURATION_MS}ms"
echo "=============================================="

# Build first
echo ""
echo "Building hiwave-smoke (release)..."
cargo build --release -p hiwave-smoke 2>&1 | tail -5

PASSED=0
FAILED=0

# Run cases based on selection
if [[ -n "$SINGLE_CASE" ]]; then
    case "$SINGLE_CASE" in
        new_tab)         run_case "new_tab" "crates/hiwave-app/src/ui/new_tab.html" 1280 800 ;;
        about)           run_case "about" "crates/hiwave-app/src/ui/about.html" 800 600 ;;
        settings)        run_case "settings" "crates/hiwave-app/src/ui/settings.html" 1024 768 ;;
        chrome_rustkit)  run_case "chrome_rustkit" "crates/hiwave-app/src/ui/chrome_rustkit.html" 1280 100 ;;
        shelf)           run_case "shelf" "crates/hiwave-app/src/ui/shelf.html" 1280 120 ;;
        article-typography) run_case "article-typography" "websuite/cases/article-typography/index.html" 1280 800 ;;
        card-grid)       run_case "card-grid" "websuite/cases/card-grid/index.html" 1280 800 ;;
        css-selectors)   run_case "css-selectors" "websuite/cases/css-selectors/index.html" 800 1200 ;;
        flex-positioning) run_case "flex-positioning" "websuite/cases/flex-positioning/index.html" 800 1000 ;;
        form-elements)   run_case "form-elements" "websuite/cases/form-elements/index.html" 800 600 ;;
        gradient-backgrounds) run_case "gradient-backgrounds" "websuite/cases/gradient-backgrounds/index.html" 800 600 ;;
        image-gallery)   run_case "image-gallery" "websuite/cases/image-gallery/index.html" 1280 800 ;;
        sticky-scroll)   run_case "sticky-scroll" "websuite/cases/sticky-scroll/index.html" 1280 800 ;;
        *)
            echo "Unknown case: $SINGLE_CASE"
            echo "Run with --help to see available cases"
            exit 1
            ;;
    esac
else
    # Run all cases
    run_case "new_tab" "crates/hiwave-app/src/ui/new_tab.html" 1280 800 && ((PASSED++)) || ((FAILED++))
    run_case "about" "crates/hiwave-app/src/ui/about.html" 800 600 && ((PASSED++)) || ((FAILED++))
    run_case "settings" "crates/hiwave-app/src/ui/settings.html" 1024 768 && ((PASSED++)) || ((FAILED++))
    run_case "chrome_rustkit" "crates/hiwave-app/src/ui/chrome_rustkit.html" 1280 100 && ((PASSED++)) || ((FAILED++))
    run_case "shelf" "crates/hiwave-app/src/ui/shelf.html" 1280 120 && ((PASSED++)) || ((FAILED++))
    run_case "article-typography" "websuite/cases/article-typography/index.html" 1280 800 && ((PASSED++)) || ((FAILED++))
    run_case "card-grid" "websuite/cases/card-grid/index.html" 1280 800 && ((PASSED++)) || ((FAILED++))
    run_case "css-selectors" "websuite/cases/css-selectors/index.html" 800 1200 && ((PASSED++)) || ((FAILED++))
    run_case "flex-positioning" "websuite/cases/flex-positioning/index.html" 800 1000 && ((PASSED++)) || ((FAILED++))
    run_case "form-elements" "websuite/cases/form-elements/index.html" 800 600 && ((PASSED++)) || ((FAILED++))
    run_case "gradient-backgrounds" "websuite/cases/gradient-backgrounds/index.html" 800 600 && ((PASSED++)) || ((FAILED++))
    run_case "image-gallery" "websuite/cases/image-gallery/index.html" 1280 800 && ((PASSED++)) || ((FAILED++))
    run_case "sticky-scroll" "websuite/cases/sticky-scroll/index.html" 1280 800 && ((PASSED++)) || ((FAILED++))
    
    echo ""
    echo "=============================================="
    echo "Results: $PASSED passed, $FAILED failed"
    echo "=============================================="
fi
