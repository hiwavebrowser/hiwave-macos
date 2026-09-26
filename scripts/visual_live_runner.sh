#!/bin/bash
# Visual Live Runner — shows LIVE websites in a RustKit window (the live-site
# twin of visual_test_runner.sh, which shows the canned fixtures).
#
# Usage: ./scripts/visual_live_runner.sh [OPTIONS]
#
# Options:
#   --site <id>           One site from the active board list (e.g. wikipedia)
#   --url <url>           Any URL (overrides --site / the list)
#   --list                Print the site ids and exit
#   --duration <ms>       How long to show each page (default: 10000)
#   --resolution <preset> fhd | macbook | laptop | ipad (default 1280x800, the board viewport)
#   --compare             Also open pinned Chrome for Testing 148 beside it, same size
#   --board <top20|wide>  Site list: top20 (default) or wide (80-site board; use with --compare)
#   --fullscreen          RustKit window fullscreen (ignored with --compare)
#
# Examples:
#   ./scripts/visual_live_runner.sh                       # all 20 board sites
#   ./scripts/visual_live_runner.sh --site wikipedia --compare
#   ./scripts/visual_live_runner.sh --compare --board wide
#   ./scripts/visual_live_runner.sh --url https://news.ycombinator.com --duration 20000
#
# Chrome for --compare: $PARITY_CHROME_PATH, else the pinned CfT under a
# .browsers/ directory next to this repo or the hiwave umbrella checkout.
# Note: RustKit does not run page JavaScript yet, so JS-rendered sites (YouTube,
# Instagram, X) show their server HTML only — that is the real-site trench's
# current target, not a runner bug.

set -u
cd "$(dirname "$0")/.."

DURATION_MS=10000
SITE=""
URL=""
COMPARE=false
FULLSCREEN=""
WIDTH=1280
HEIGHT=800
LIST="websuite/realsite-top20.json"
LIST_WIDE="websuite/realsite-top80.json"
BOARD="top20"
LIST_ONLY=false

declare -a RESOLUTIONS=(
    "fhd:1920:1080"
    "macbook:1440:900"
    "laptop:1366:768"
    "ipad:1024:768"
)

usage() { sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'; }

while [[ $# -gt 0 ]]; do
    case $1 in
        --site) SITE="$2"; shift 2 ;;
        --url) URL="$2"; shift 2 ;;
        --duration) DURATION_MS="$2"; shift 2 ;;
        --compare) COMPARE=true; shift ;;
        --board)
            BOARD="$2"
            shift 2 ;;
        --fullscreen) FULLSCREEN="--fullscreen"; shift ;;
        --resolution)
            found=""
            for res in "${RESOLUTIONS[@]}"; do
                IFS=':' read -r name w h <<< "$res"
                if [[ "$name" == "$2" ]]; then WIDTH=$w; HEIGHT=$h; found=1; fi
            done
            [[ -z "$found" ]] && { echo "Unknown resolution preset: $2"; exit 1; }
            shift 2 ;;
        --list) LIST_ONLY=true; shift ;;
        --help|-h) usage; exit 0 ;;
        *) echo "Unknown option: $1"; usage; exit 1 ;;
    esac
done

case "$BOARD" in
    top20) ;;
    wide) LIST="$LIST_WIDE" ;;
    *) echo "Unknown board: $BOARD (use top20 or wide)"; exit 1 ;;
esac
if [[ "$BOARD" == "wide" ]] && ! $COMPARE; then
    echo "Note: --board wide is intended for --compare runs over the extended site list."
fi

if $LIST_ONLY; then
    python3 -c "import json;[print(f\"{s['id']:<12} {s['url']}\") for s in json.load(open('$LIST'))['sites']]"
    exit 0
fi

# Build the list of (id url) pairs to show.
declare -a TARGETS=()
if [[ -n "$URL" ]]; then
    TARGETS+=("custom $URL")
elif [[ -n "$SITE" ]]; then
    line=$(python3 -c "import json,sys;m=[s for s in json.load(open('$LIST'))['sites'] if s['id']=='$SITE'];print(m[0]['url'] if m else '')")
    [[ -z "$line" ]] && { echo "Unknown site: $SITE (use --list)"; exit 1; }
    TARGETS+=("$SITE $line")
else
    while IFS= read -r l; do TARGETS+=("$l"); done < <(python3 -c "import json;[print(s['id'],s['url']) for s in json.load(open('$LIST'))['sites']]")
fi

CHROME=""
if $COMPARE; then
    CHROME="${PARITY_CHROME_PATH:-}"
    if [[ -z "$CHROME" ]]; then
        for root in ".browsers" "../hiwave/hiwave-macos/.browsers" "$HOME/Repos/hiwave/hiwave-macos/.browsers"; do
            c=$(ls -d "$root"/chrome/mac_arm-148.*/chrome-mac-arm64/"Google Chrome for Testing.app"/Contents/MacOS/"Google Chrome for Testing" 2>/dev/null | head -1)
            [[ -n "$c" ]] && { CHROME="$c"; break; }
        done
    fi
    [[ -x "$CHROME" ]] || { echo "--compare: pinned Chrome for Testing not found (set PARITY_CHROME_PATH)"; exit 1; }
    FULLSCREEN=""
fi

echo "=============================================="
echo "Visual Live Runner — ${#TARGETS[@]} site(s), ${WIDTH}x${HEIGHT}, ${DURATION_MS}ms each"
$COMPARE && echo "Comparing against: $(basename "$(dirname "$(dirname "$(dirname "$CHROME")")")")"
echo "=============================================="
echo "Building hiwave-smoke (release)..."
cargo build --release -p hiwave-smoke 2>&1 | tail -2

SHOWN=0; FAILED=0
for t in "${TARGETS[@]}"; do
    id=${t%% *}; url=${t#* }
    echo ""
    echo "--- $id — $url ---"
    CPID=""
    if $COMPARE; then
        prof=$(mktemp -d /tmp/visual-live-chrome.XXXX)
        "$CHROME" --user-data-dir="$prof" --no-first-run --no-default-browser-check \
            --lang=en-US --force-color-profile=srgb \
            --window-size="$WIDTH,$HEIGHT" --window-position="$((WIDTH + 20)),40" \
            --app="$url" >/dev/null 2>&1 &
        CPID=$!
    fi
    log=$(mktemp /tmp/visual-live-smoke.XXXX)
    ./target/release/hiwave-smoke --url "$url" --width "$WIDTH" --height "$HEIGHT" \
        --duration-ms "$DURATION_MS" $FULLSCREEN >"$log" 2>&1
    status=$?
    grep -E 'ERROR|Failed' "$log" | head -3
    rm -f "$log"
    if [[ -n "$CPID" ]]; then kill "$CPID" 2>/dev/null; wait "$CPID" 2>/dev/null; rm -rf "$prof"; fi
    if [[ "$status" -eq 0 ]]; then echo "  ✓ shown"; SHOWN=$((SHOWN+1)); else echo "  ✗ exit $status"; FAILED=$((FAILED+1)); fi
done

echo ""
echo "=============================================="
echo "Shown: $SHOWN, errors: $FAILED"
echo "=============================================="
