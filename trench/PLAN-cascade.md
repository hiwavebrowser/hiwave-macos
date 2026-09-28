# Cascade-speed trench — PLAN

Owner: Atlas (headless trench sessions, macOS seat). Reviewers: Prometheus (R1), Cursor R2 bot. Prometheus or Atlas merges; a trench session NEVER merges.

## Phase 0 — the instrument (first session)
1. Find where RustKit already times the cascade (engine logs, parity-capture timing output). USE ALEPH before grep/read. If there is no per-load style timer, add one behind an env var or a parity-capture flag. Keep it cheap and off by default.
2. Write `trench/tools/cascade_bench.sh` (or `.py`). It runs the release parity-capture on the pinned cnn, github and wikipedia snapshots, 5 times each, and prints the median and raw style time per site, plus the ratio against the Chrome numbers in the BASELINE.
3. Measure develop, and replace the "Starting value" table in BASELINE-cascade.md with the measured one. Commit it on the hub branch.

## Phase 1 — grind the ratio, in this order (Blink ideas, never Blink code)
1. **Ancestor Bloom filter** in `rule_may_match`. Descendant and child combinators get fast-rejected when the ancestor's id, class or tag isn't present.
2. **Fast reject** for rules whose rightmost compound can never match this document (index by the rightmost id, class or tag; most CSS is unused, with a median of 12.9% used).
3. **Matched-properties cache.** Elements with the same matched rule set and the same parent style share the computed style.
4. **No full re-cascade per relayout.** Today there are 2–3 full cascades per load. Cascade once, and restyle only dirty subtrees afterwards (incremental invalidation, first cut).
Re-measure after each PR and put the number in the PR body.

## Rules
- One metric. Correctness first: the builtins parity scope, rustkit tests and the real-site board must not regress. Put the campaign receipt (builtins scope) in every PR body, or R2 fails gate 5.
- Branches: `atlas/cs-<slug>` from origin/develop, each in its own git worktree under ~/Repos/.worktrees/. NEVER touch ~/Repos/hiwave-macos or the real-site lane's worktrees.
- Build with CARGO_TARGET_DIR=~/Repos/.worktrees/cascade-target (the lane's own target dir, so it doesn't fight the real-site lane's builds).
- Small, test-passing commits. Never force-push a reviewed branch. Rebase only when CONFLICTING; prefer an additive merge of develop.
- Coordinate with the real-site lane through this file and the digest, not by editing its hub.
