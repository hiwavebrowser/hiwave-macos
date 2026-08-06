#!/usr/bin/env bash
# wpt_sync.sh — materialise ONLY the tests listed in trench/wpt/MANIFEST.json into third_party/wpt/.
#
# The manifest is the source of truth; this script is a projection of it. It never adds a path the
# manifest does not list, and it never vendors the full WPT tree (~500k files) into the monorepo.
#
#   ./scripts/wpt_sync.sh            # sync to the pinned SHA
#   ./scripts/wpt_sync.sh --check    # verify every manifest path exists locally; no network, no writes
#   ./scripts/wpt_sync.sh --dry-run  # print what would be fetched, touch nothing
#
# NOT YET RUN ON ANY SEAT. The macOS trench seat's Bash allowlist has no git clone / curl, so this
# ships un-exercised (trench/wpt/README.md, known gap 1). Treat the first real run as W0b's first task
# and expect to fix something here.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$REPO_ROOT/trench/wpt/MANIFEST.json"
DEST="$REPO_ROOT/third_party/wpt"
WPT_REPO="https://github.com/web-platform-tests/wpt"

MODE="sync"
case "${1:-}" in
  --check)   MODE="check" ;;
  --dry-run) MODE="dry-run" ;;
  "")        ;;
  *) echo "unknown argument: $1" >&2; exit 2 ;;
esac

[ -f "$MANIFEST" ] || { echo "manifest not found: $MANIFEST" >&2; exit 1; }

PIN="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["wpt_pin"])' "$MANIFEST")"
# Both test and ref paths — a reftest without its reference is not a test.
PATHS="$(python3 -c '
import json, sys
m = json.load(open(sys.argv[1]))
out = []
for e in m["entries"]:
    out.append(e["path"])
    if e.get("ref"):
        out.append(e["ref"])
# Shared support files (fonts served from / by wptserve). Not tests, but a
# reftest whose font never loads is not measuring what it asserts.
out.extend(m.get("support_paths", {}).get("paths", []))
print("\n".join(sorted(set(out))))
' "$MANIFEST")"

N="$(printf "%s\n" "$PATHS" | grep -c . || true)"
echo "manifest: $MANIFEST"
echo "pin:      $PIN"
echo "files:    $N (tests + refs + support)"
echo "dest:     $DEST"

if [ "$MODE" = "check" ]; then
  missing=0
  while IFS= read -r p; do
    [ -n "$p" ] || continue
    if [ ! -f "$DEST/$p" ]; then echo "MISSING $p"; missing=$((missing + 1)); fi
  done <<< "$PATHS"
  if [ "$missing" -gt 0 ]; then
    echo "FAIL: $missing/$N manifest paths absent from $DEST — run without --check to sync." >&2
    exit 1
  fi
  echo "OK: all $N manifest paths present at $DEST"
  exit 0
fi

if [ "$MODE" = "dry-run" ]; then
  echo "--- would sparse-checkout these paths at $PIN ---"
  printf "%s\n" "$PATHS"
  exit 0
fi

# Sparse checkout: fetch the pinned commit's tree, populate only the manifest paths.
mkdir -p "$DEST"
if [ ! -d "$DEST/.git" ]; then
  git -C "$DEST" init -q
  git -C "$DEST" remote add origin "$WPT_REPO"
  git -C "$DEST" config core.sparseCheckout true
  git -C "$DEST" sparse-checkout init --no-cone
fi

# --no-cone takes literal path patterns, which is exactly the manifest's shape.
printf "%s\n" "$PATHS" | git -C "$DEST" sparse-checkout set --no-cone --stdin

git -C "$DEST" fetch --depth 1 origin "$PIN"
git -C "$DEST" checkout -q FETCH_HEAD

# Sync is not done until every listed path is on disk — a partial checkout that reports success is the
# same class of lie as an empty capture scored as 100% (forensics 2026-07-24).
exec "$0" --check
