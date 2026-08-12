# Outside-eye R1: hiwave-macos PR #99 (W0b seed expand / parallel harness)

**Date:** 2026-08-05  
**Seat:** Prometheus (design / outside-eye)  
**PR:** https://github.com/hiwavebrowser/hiwave-macos/pull/99  
**Tip:** `69a5ac2` (`atlas/wpt-w0b`)  
**Base claimed:** master (mergeable **CONFLICTING**)  
**Master at review:** `ceac6bb` (#98 promote MERGED)  
**Merge-base tip↔master:** `fd7e4c0` (pre-#74 W0b land)

## Verdict

| Item | Ruling |
|------|--------|
| #99 as-is merge (replace `wpt_tier1.py` + last-run) | **REJECT / SUPERSEDE** |
| Master W0b harness (#74 `15e4cff` + #77 residual clear `3db1783`) | **AUTHORITATIVE** |
| Blank-frame gate (blank ≠ match → ERROR) | **HARD KEEP** — tip drops it |
| Negative control every run | **HARD KEEP** — tip has **zero** reference |
| Seed expand 14→30 + scout + content-blind merge scripts | **SALVAGE** onto master harness |
| Tip headline `8/26 (30.8%)` as fleet SoT | **REJECT** until re-run under master harness |
| Merge / force-push / close | **Atlas / Pete** — not Prometheus |

## Live board (this tick)

| Surface | Tip / state |
|---------|-------------|
| macOS **#99** | tip **`69a5ac2`** · OPEN · **CONFLICTING** · collect-metrics **SUCCESS** only |
| macOS master | **`ceac6bb`** (#98 promote MERGED; W0b already on master via #74/#77) |
| macOS develop | **`6c7ef42`** |
| Linux **#59** | OPEN · Argos R1 GREEN banked @ `c0701ae` (not this unit) |
| Linux **#58** | OPEN · Prometheus CLEAR banked @ `387a8ee` |
| Win | open **#33 HOLD only** · #78/#79 MERGED→develop |
| umbrella **#11** | OPEN · HARD AMEND banked · tip moved `62a58d4` |
| tank / community | open **zero** |

## Independent ground

Worktrees: `/tmp/hiwave-pr99-r1` @ `69a5ac2` · `/tmp/hiwave-pr99-master` @ `ceac6bb`.

### Scope

| Path | Tip delta vs master (pre-conflict) |
|------|-------------------------------------|
| `scripts/wpt_tier1.py` | **add/add CONFLICT** — tip rewrites already-landed harness |
| `trench/wpt/last-run.json` | **add/add CONFLICT** |
| `trench/wpt/MANIFEST.json` | tip expands entries **14 → 30** (auto-merges on trial; not the conflict pair) |
| `scripts/wpt_seed_scout.py` | new |
| `scripts/wpt_seed_merge_1a.py` | new |
| `scripts/wpt_sync.py` | new (python twin of `wpt_sync.sh`) |
| `trench/wpt/seed-scout-1A.json` | new census receipt |

Trial merge on master: conflicts **only** in `scripts/wpt_tier1.py` + `trench/wpt/last-run.json`.

### Master already shipped W0b

| Commit | What |
|--------|------|
| `#74` `15e4cff` | First honest Tier-1 K/N: harness + negative control + blank→ERROR + last-run |
| `#77` `3db1783` | Honesty flag computed; last-run names landed SHA |
| Pin doc | `docs/WPT_W0B_IMPLEMENT_PIN_2026-07-29.md` on master |

Master last-run (authoritative shape):

- `schema: 1`, `hiwave_git_sha`, `wpt_max_diff_pct: 0.0`
- `n=14` · pass 6 · fail 6 · error 2 · rate 0.5
- `honesty.negative_control` required FAIL every run
- Status set: PASS / FAIL / ERROR (blank / capture)

Tip last-run (weaker shape):

- no schema · no honesty block · no negative_control field
- `n=26` · pass 8 · fail 18 · skip 4 · instrument 0 · rate_pct 30.8
- blankness **recorded only**, never judged

### Proven regression (shared case)

| Case | Master | Tip | Evidence |
|------|--------|-----|----------|
| `css-flexbox/align-items-baseline-overflow-non-visible` | **ERROR** (blank render refusal) | **PASS** (diff 0) | tip `test_blank_ratio=0.9992` / `ref_blank_ratio=0.9992` |

This is the empty-capture-scores-100 lie class wearing a reftest costume — the exact class master #74 closed. Tip docstring admits equal-blank can be a lying pass and **chooses not to gate**.

Additional status flips (shared seed):

| Case | Master | Tip |
|------|--------|-----|
| `empty-span-height` | FAIL | SKIP (reftest-wait) |
| `empty-span-scroll` | ERROR | SKIP (reftest-wait) |
| `empty-span-size-001` | FAIL | SKIP (reftest-wait) |
| `empty-text-node-001` | FAIL | SKIP (needs JS) |

SKIP for JS/reftest-wait is a reasonable *future* policy discussion, but it must not land by silently replacing the master harness mid-conflict. Product residual today is the blank→PASS regression.

### What is worth salvaging

1. **Seed growth rule** — content-blind round-robin across 1A dirs (`wpt_seed_merge_1a.py`) is the right anti-cherry-pick shape.
2. **Scout** — `wpt_seed_scout.py` + `seed-scout-1A.json` (fixed `.group(1)` / same-dir ref bugs) are useful census tools.
3. **MANIFEST expansion** — 16 new 1A cases; tip-only results under tip harness are mostly FAIL (honest pressure), but K/N is not publishable until master blank gate + negative control re-score them.
4. **`wpt_sync.py`** — optional twin if it preserves master's receipt contract (`--check` / pin); must not orphan `wpt_sync.sh` without a migration note.

### What must not land from tip

1. Replace master `wpt_tier1.py` (drops negative control + blank gate).
2. Replace master `last-run.json` as the published meter without re-run under master harness.
3. Claim "first honest Tier-1" — master already owns that title via #74.

## Rulings (frozen)

1. **Master harness is SoT.** Any seed expand PR takes master's `scripts/wpt_tier1.py` and negative-control fixtures as non-negotiable.
2. **#99 REJECT as-is.** Close or HARD AMEND: rebase onto `ceac6bb` (or later master); **ours = master** for `wpt_tier1.py`; keep tip scout/merge/manifest deltas; re-run tier1; commit new last-run with master honesty schema.
3. **Blank gate stays HARD.** `is_blank` on test or ref → ERROR (or equivalent non-score refusal), never PASS via exact match of two blanks.
4. **Negative control stays HARD.** Every published run must fail the deliberate mismatch first or abort.
5. **SKIP policy** for reftest-wait / JS is a **separate design unit** — do not smuggle via harness rewrite.
6. **Prometheus does not merge / force-push / close.**

## Atlas execute (when capacity)

```text
1. git fetch && checkout -B atlas/wpt-w0b-salvage origin/master
2. cherry-pick or manually port ONLY:
     scripts/wpt_seed_scout.py
     scripts/wpt_seed_merge_1a.py
     scripts/wpt_sync.py          # optional
     trench/wpt/seed-scout-1A.json
     trench/wpt/MANIFEST.json     # entries expand; keep pin/viewport
3. DO NOT take tip wpt_tier1.py or tip last-run.json
4. wpt_sync --check (or sh) at pin a6f29b0
5. python3 scripts/wpt_tier1.py   # master harness
6. commit last-run.json + manifest; open new PR or HARD AMEND #99
7. falsifier: align-items-baseline-overflow-non-visible must not be PASS if blank
```

## Argos falsifiers (if re-R1)

- Tip/master merge must not delete `NEGATIVE_CONTROL_DIR` usage.
- `rg 'blank frame' scripts/wpt_tier1.py` still hits a hard gate, not a comment-only record.
- Shared case `align-items-baseline-overflow-non-visible` status ≠ PASS when both blank ratios ~1.0.
- last-run retains `honesty` / negative_control receipt fields (schema ≥ master #77).

## Prometheus next

Outside-eye first *new* tip after this bank (Linux #59 only if tip moves past Argos GREEN · Win tip move · umbrella #11 only if HARD AMEND residual re-opens · paint trade when Pete opens). Do **not** re-pin #99 SUPERSEDE · #98 MERGED · #58 CLEAR · #33 HOLD · #11 HARD AMEND unless measurement changes.

## No irreversible

No merge, force-push, master write, spend, or `null attend` from this seat.
