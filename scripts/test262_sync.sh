#!/usr/bin/env bash
# test262_sync.sh — fetch tc39/test262 at the pinned commit into third_party/test262/ (not checked in).
#
# The pin is in trench/test262/PIN. The suite (54k files, BSD-3-Clause) is never vendored; this
# script is the only thing that writes third_party/test262/.
#
#   ./scripts/test262_sync.sh            # fetch or update to the pin
#   ./scripts/test262_sync.sh --check    # verify the checkout is at the pin; no network, no writes

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PIN="$(tr -d '[:space:]' < "$REPO_ROOT/trench/test262/PIN")"
DEST="$REPO_ROOT/third_party/test262"
URL="https://github.com/tc39/test262"

have() { [ -d "$DEST/.git" ] && [ "$(git -C "$DEST" rev-parse HEAD 2>/dev/null)" = "$PIN" ]; }

if [ "${1:-}" = "--check" ]; then
  have || { echo "third_party/test262 is not at $PIN; run scripts/test262_sync.sh" >&2; exit 1; }
  echo "test262 at $PIN"
  exit 0
fi
[ -z "${1:-}" ] || { echo "unknown argument: $1" >&2; exit 2; }

if have; then echo "test262 already at $PIN"; exit 0; fi
mkdir -p "$DEST"
[ -d "$DEST/.git" ] || git -C "$DEST" init -q
git -C "$DEST" -c core.autocrlf=false fetch -q --depth 1 "$URL" "$PIN"
git -C "$DEST" -c core.autocrlf=false checkout -q --detach FETCH_HEAD
have || { echo "checkout is not at the pin" >&2; exit 1; }
echo "test262 at $PIN"
