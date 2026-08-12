#!/bin/bash
# pete-session.sh — human-in-the-loop test session with artifacts, not anecdotes.
#
# Usage: ./scripts/pete-session.sh [--release]
#
# Launches HiWave headed with structured logging, snapshots workspace state
# before and after, and on exit auto-triages the log into error classes.
# Pete drives; the session directory is what Atlas reads.
#
# Everything lands in: hiwave-sessions/<timestamp>/
#   run.log            full RUST_LOG output
#   triage.md          error classes with counts, first-seen, per-view
#   workspace_before/after.json   tab state snapshots
#   meta.txt           commit, build flags, duration

set -u
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$SCRIPT_DIR/.." && pwd)"
TS=$(date +%Y%m%d_%H%M%S)
OUT="$REPO/hiwave-sessions/$TS"
mkdir -p "$OUT"

PROFILE_FLAG=""
PROFILE="debug"
VERBOSE=0
for arg in "$@"; do
  case "$arg" in
    --release) PROFILE_FLAG="--release"; PROFILE="release" ;;
    --verbose) VERBOSE=1 ;;
    *) echo "unknown flag: $arg (want --release and/or --verbose)" >&2; exit 2 ;;
  esac
done

STATE="$HOME/Library/Application Support/hiwave/workspace_state.json"
[ -f "$STATE" ] && cp "$STATE" "$OUT/workspace_before.json"

{
  echo "commit: $(git -C "$REPO" rev-parse --short HEAD) ($(git -C "$REPO" branch --show-current))"
  echo "profile: $PROFILE"
  echo "started: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > "$OUT/meta.txt"

echo "── HiWave session $TS ──"
echo "   log: $OUT/run.log   (Atlas can tail this live)"
START=$(date +%s)

# Log filter. The binary's target is `hiwave` (bin name), NOT `hiwave_app`
# (crate name) — filtering on the crate name silently drops every shell line,
# which is how a whole layer goes missing from a session log.
#
# `--verbose` drops every RustKit crate to debug and turns on the input trace
# lines (wheel/scroll/key). Those are trace-level on purpose: they fire per
# event and would drown a normal session. But when the question IS input, a
# session without them cannot answer it — 2026-08-06, where "no wheel events
# in the log" could not be told apart from "the log does not carry wheel
# events."
if [ "$VERBOSE" -eq 1 ]; then
  RUST_LOG_SPEC="warn,hiwave=trace,rustkit_engine=debug,rustkit_renderer=debug,rustkit_layout=debug,rustkit_css=debug,rustkit_net=debug,rustkit_viewhost=debug,rustkit_compositor=debug,rustkit_text=debug"
  echo "   verbose: RustKit crates at debug, shell input trace ON"
else
  RUST_LOG_SPEC="warn,hiwave=info,rustkit_engine=info,rustkit_renderer=info,rustkit_core=info"
fi
echo "   RUST_LOG=$RUST_LOG_SPEC" >> "$OUT/meta.txt"

RUST_LOG="$RUST_LOG_SPEC" \
  cargo run -p hiwave-app --features rustkit $PROFILE_FLAG 2>&1 | tee "$OUT/run.log"
APP_EXIT=${PIPESTATUS[0]}

END=$(date +%s)
{
  echo "ended: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "duration_s: $((END-START))"
  echo "app_exit: $APP_EXIT"
} >> "$OUT/meta.txt"
[ -f "$STATE" ] && cp "$STATE" "$OUT/workspace_after.json"

# ── auto-triage ──────────────────────────────────────────────────────────
python3 - "$OUT" <<'PY'
import re, sys, collections, pathlib
out = pathlib.Path(sys.argv[1])
log = (out/"run.log").read_text(errors="replace").splitlines()

classes = collections.Counter()
first = {}
views = collections.Counter()
samples = {}
for i, l in enumerate(log):
    lvl = "ERROR" if "ERROR" in l else ("WARN" if "WARN" in l else None)
    if not lvl:
        continue
    # class = the message with numbers/ids/urls stripped, so counts group
    msg = re.sub(r'\x1b\[[0-9;]*m', '', l)
    msg = msg.split(lvl, 1)[-1].strip(": ").strip()
    key = re.sub(r'\d+', 'N', re.sub(r'(url|id)=\S*', r'\1=…', msg))[:110]
    key = f"{lvl} {key}"
    classes[key] += 1
    first.setdefault(key, i + 1)
    samples.setdefault(key, msg[:160])
    m = re.search(r'EngineViewId\((\d+)\)', l)
    if m:
        views[m.group(1)] += 1

with (out/"triage.md").open("w") as f:
    f.write(f"# Session triage — {out.name}\n\n")
    f.write(f"log lines: {len(log)} · error/warn lines: {sum(classes.values())} "
            f"· distinct classes: {len(classes)}\n\n")
    f.write("| count | first@line | class |\n|---|---|---|\n")
    for k, v in classes.most_common(30):
        f.write(f"| {v} | {first[k]} | `{k}` |\n")
    if views:
        f.write("\n## error/warn lines by view\n")
        for vid, n in views.most_common():
            f.write(f"- EngineViewId({vid}): {n}\n")
    f.write("\n## first sample per class\n")
    for k, _ in classes.most_common(30):
        f.write(f"- {samples[k]}\n")
print(f"   triage: {out/'triage.md'}")
PY

echo "── session complete: $OUT ──"
