#!/bin/bash
# fetch.sh — check the pinned benchmark suites out at the shas in pins.json.
# The only network use in tools/stress: three clones from github.com/WebKit.
# Suites land in $STRESS_SUITES (default ~/zg-measure/stress-suites), never
# inside this repository. Re-running verifies an existing checkout and puts
# it back on its pin; it fetches only when the pinned commit is missing.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
dest=${STRESS_SUITES:-$HOME/zg-measure/stress-suites}
mkdir -p "$dest"
python3 - "$here/pins.json" <<'PY' | while IFS=$'\t' read -r dir repo sha; do
import json, sys
for s in json.load(open(sys.argv[1]))["suites"].values():
    print(s["dir"], s["repo"], s["sha"], sep="\t")
PY
  d="$dest/$dir"
  if [ ! -d "$d/.git" ]; then
    git clone --quiet --no-checkout "$repo" "$d"
  fi
  if ! git -C "$d" cat-file -e "$sha^{commit}" 2>/dev/null; then
    git -C "$d" fetch --quiet origin "$sha" || git -C "$d" fetch --quiet origin
  fi
  git -C "$d" checkout --quiet --detach "$sha"
  if [ -n "$(git -C "$d" status --porcelain)" ]; then
    echo "DIRTY  $dir: the checkout differs from $sha (git -C $d status)" >&2
    exit 1
  fi
  echo "ok  $dir  $(git -C "$d" rev-parse HEAD)"
done
