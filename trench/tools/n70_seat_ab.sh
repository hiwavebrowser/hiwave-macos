#!/usr/bin/env bash
# Capture the 26 gating cases with the release `parity-capture` and run Gate A
# and Gate B over them. NOT A RECEIPT — this seat is SwiftShader and the Linux
# font stack, not Metal and CoreText. Mechanics only.
#
#   n70_seat_ab.sh <out-dir>
set -euo pipefail
OUT="${1:?usage: n70_seat_ab.sh <out-dir>}"
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
export VK_ICD_FILENAMES="${VK_ICD_FILENAMES:-/opt/pw-browsers/chromium-1194/chrome-linux/vk_swiftshader_icd.json}"
mkdir -p "$OUT"
python3 - "$REPO" <<'PY' > "$OUT/cases.tsv"
import json, sys
cases = json.load(open(sys.argv[1] + "/cases/registry.json"))["cases"]
for k, v in cases.items():
    if v.get("scope") == "holdout":
        continue
    print(f"{k}\t{v['html']}\t{v['width']}\t{v['height']}")
PY
while IFS=$'\t' read -r name html w h; do
  mkdir -p "$OUT/captures/$name"
  "$REPO/target/release/parity-capture" \
      --html-file "$REPO/$html" --width "$w" --height "$h" \
      --dump-layout "$OUT/captures/$name/layout.json" \
      --dump-frame "$OUT/captures/$name/frame.ppm" \
      > "$OUT/captures/$name/result.json" 2> "$OUT/captures/$name/stderr.txt" \
    || echo "  capture FAILED: $name (see $OUT/captures/$name/stderr.txt)" >&2
done < "$OUT/cases.tsv"
python3 "$REPO/scripts/layout_oracle_gate.py" --layout-root "$OUT/captures" \
    --json "$OUT/gate-a.json" > "$OUT/gate-a.txt" 2>&1 || true
python3 "$REPO/scripts/paint_oracle_gate.py" --capture-root "$OUT/captures" \
    --json "$OUT/gate-b.json" > "$OUT/gate-b.txt" 2>&1 || true
tail -14 "$OUT/gate-a.txt"
tail -14 "$OUT/gate-b.txt"
