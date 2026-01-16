#!/bin/bash
# Visual Test Runner - Shows each parity test case in a window
# Usage: ./scripts/visual_test_runner.sh [OPTIONS]
#
# Options:
#   --duration <ms>       How long to show each page (default: 3000)
#   --case <name>         Run only a single case
#   --fullscreen          Run tests in fullscreen mode
#   --resolution <preset> Test at specific resolution (see below)
#   --all-resolutions     Run each test at all standard resolutions
#
# Resolution presets:
#   fhd      - 1920x1080 (Full HD - most common desktop)
#   macbook  - 1440x900  (MacBook Pro 13")
#   qhd      - 2560x1440 (QHD/2K)
#   laptop   - 1366x768  (Common laptop)
#   ipad     - 1024x768  (iPad landscape)
#   mobile   - 414x896   (iPhone 11 Pro Max)

set -e

DURATION_MS=3000
SINGLE_CASE=""
FULLSCREEN=""
RESOLUTION=""
ALL_RESOLUTIONS=false

# Standard test resolutions (name:width:height)
declare -a RESOLUTIONS=(
    "fhd:1920:1080"
    "macbook:1440:900"
    "laptop:1366:768"
    "ipad:1024:768"
)

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
        --fullscreen)
            FULLSCREEN="--fullscreen"
            shift
            ;;
        --resolution)
            RESOLUTION="$2"
            shift 2
            ;;
        --all-resolutions)
            ALL_RESOLUTIONS=true
            shift
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --duration <ms>       How long to show each page (default: 3000)"
            echo "  --case <name>         Run only a single case"
            echo "  --fullscreen          Run tests in fullscreen mode"
            echo "  --resolution <preset> Test at specific resolution"
            echo "  --all-resolutions     Run each test at all standard resolutions"
            echo ""
            echo "Resolution presets:"
            echo "  fhd      - 1920x1080 (Full HD)"
            echo "  macbook  - 1440x900  (MacBook Pro 13\")"
            echo "  laptop   - 1366x768  (Common laptop)"
            echo "  ipad     - 1024x768  (iPad landscape)"
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

# Get resolution dimensions from preset
get_resolution() {
    local preset="$1"
    for res in "${RESOLUTIONS[@]}"; do
        IFS=':' read -r name width height <<< "$res"
        if [[ "$name" == "$preset" ]]; then
            echo "$width:$height"
            return 0
        fi
    done
    echo ""
    return 1
}

run_case() {
    local name="$1"
    local html="$2"
    local width="$3"
    local height="$4"
    local res_label="$5"

    if [[ -n "$res_label" ]]; then
        echo ""
        echo "--- $name @ $res_label (${width}x${height}) ---"
    else
        echo ""
        echo "--- $name ---"
    fi
    echo "  File: $html"
    echo "  Size: ${width}x${height}"
    if [[ -n "$FULLSCREEN" ]]; then
        echo "  Mode: fullscreen"
    fi
    echo "  Opening window for ${DURATION_MS}ms..."

    if cargo run --release -p hiwave-smoke -- \
        --html-file "$html" \
        --width "$width" \
        --height "$height" \
        --duration-ms "$DURATION_MS" \
        $FULLSCREEN 2>&1; then
        echo "  ✓ Done"
        return 0
    else
        echo "  ✗ Error occurred"
        return 1
    fi
}

# Run a case at multiple resolutions
run_case_all_resolutions() {
    local name="$1"
    local html="$2"
    local default_width="$3"
    local default_height="$4"
    local local_passed=0
    local local_failed=0

    for res in "${RESOLUTIONS[@]}"; do
        IFS=':' read -r res_name width height <<< "$res"
        if run_case "$name" "$html" "$width" "$height" "$res_name"; then
            ((local_passed++))
        else
            ((local_failed++))
        fi
    done

    echo "  Resolutions: $local_passed passed, $local_failed failed"
    [[ $local_failed -eq 0 ]]
}

echo "=============================================="
echo "Visual Test Runner"
echo "Duration per case: ${DURATION_MS}ms"
if [[ -n "$FULLSCREEN" ]]; then
    echo "Mode: Fullscreen"
fi
if [[ -n "$RESOLUTION" ]]; then
    echo "Resolution: $RESOLUTION"
fi
if [[ "$ALL_RESOLUTIONS" == true ]]; then
    echo "Testing all resolutions"
fi
echo "=============================================="

# Build first
echo ""
echo "Building hiwave-smoke (release)..."
cargo build --release -p hiwave-smoke 2>&1 | tail -5

PASSED=0
FAILED=0

# Determine width and height
WIDTH=1280
HEIGHT=800

if [[ -n "$RESOLUTION" ]]; then
    res_dims=$(get_resolution "$RESOLUTION")
    if [[ -n "$res_dims" ]]; then
        IFS=':' read -r WIDTH HEIGHT <<< "$res_dims"
    else
        echo "Unknown resolution preset: $RESOLUTION"
        echo "Available presets: fhd, macbook, laptop, ipad"
        exit 1
    fi
fi

# Test case definitions (name:html:default_width:default_height)
declare -a CASES=(
    "new_tab:crates/hiwave-app/src/ui/new_tab.html:1280:800"
    "about:crates/hiwave-app/src/ui/about.html:800:600"
    "settings:crates/hiwave-app/src/ui/settings.html:1024:768"
    "chrome_rustkit:crates/hiwave-app/src/ui/chrome_rustkit.html:1280:100"
    "shelf:crates/hiwave-app/src/ui/shelf.html:1280:120"
    "article-typography:websuite/cases/article-typography/index.html:1280:800"
    "card-grid:websuite/cases/card-grid/index.html:1280:800"
    "css-selectors:websuite/cases/css-selectors/index.html:800:1200"
    "flex-positioning:websuite/cases/flex-positioning/index.html:800:1000"
    "form-elements:websuite/cases/form-elements/index.html:800:600"
    "gradient-backgrounds:websuite/cases/gradient-backgrounds/index.html:800:600"
    "image-gallery:websuite/cases/image-gallery/index.html:1280:800"
    "sticky-scroll:websuite/cases/sticky-scroll/index.html:1280:800"
)

# Find a case by name
find_case() {
    local search="$1"
    for case_def in "${CASES[@]}"; do
        IFS=':' read -r name html default_w default_h <<< "$case_def"
        if [[ "$name" == "$search" ]]; then
            echo "$name:$html:$default_w:$default_h"
            return 0
        fi
    done
    return 1
}

# Run cases based on selection
if [[ -n "$SINGLE_CASE" ]]; then
    case_def=$(find_case "$SINGLE_CASE")
    if [[ -z "$case_def" ]]; then
        echo "Unknown case: $SINGLE_CASE"
        echo "Run with --help to see available cases"
        exit 1
    fi

    IFS=':' read -r name html default_w default_h <<< "$case_def"

    if [[ "$ALL_RESOLUTIONS" == true ]]; then
        run_case_all_resolutions "$name" "$html" "$default_w" "$default_h"
    else
        # Use specified resolution or default
        if [[ -n "$RESOLUTION" ]]; then
            run_case "$name" "$html" "$WIDTH" "$HEIGHT" "$RESOLUTION"
        else
            run_case "$name" "$html" "$default_w" "$default_h"
        fi
    fi
else
    # Run all cases
    for case_def in "${CASES[@]}"; do
        IFS=':' read -r name html default_w default_h <<< "$case_def"

        if [[ "$ALL_RESOLUTIONS" == true ]]; then
            if run_case_all_resolutions "$name" "$html" "$default_w" "$default_h"; then
                ((PASSED++))
            else
                ((FAILED++))
            fi
        else
            # Use specified resolution or default
            if [[ -n "$RESOLUTION" ]]; then
                test_w="$WIDTH"
                test_h="$HEIGHT"
            else
                test_w="$default_w"
                test_h="$default_h"
            fi

            if run_case "$name" "$html" "$test_w" "$test_h"; then
                ((PASSED++))
            else
                ((FAILED++))
            fi
        fi
    done

    echo ""
    echo "=============================================="
    echo "Results: $PASSED passed, $FAILED failed"
    echo "=============================================="
fi
