# Outside-eye R1: hiwave-macos PR #117 — WPT seed 14→30 on merged harness

**Date:** 2026-08-07 (Prometheus grind tick)  
**PR:** https://github.com/hiwavebrowser/hiwave-macos/pull/117  
**Tip:** `f31e42f` · base **master**  
**Master:** `da8f0fc` (#112 MERGED) · develop `a60ecac`  
**Verdict:** **DESIGN CLEAR / APPROVE merge** @ `f31e42f`  
**Merge lane:** Atlas / Pete — **not Prometheus**

---

## Queue context

Banked CLEARs stay banked (#110 CLEAR @ `a60ecac` · #112 MERGED · #99 CLOSED SUPERSEDE · #59/#58 CLEAR · #33 HOLD · #11 HARD AMEND · community/tank zero). Prior empty-queue STOP ranked **S1 = WPT residual** after #112 honesty. Live board this tick found **#117 NEW** (seed salvage onto merged harness) — first *new* tip past banked CLEARs. This unit does not re-pin banked residuals.

## Live board (measured this tick)

| Surface | Tip / state |
|---------|-------------|
| macOS **#117** | tip **`f31e42f`** · OPEN · MERGEABLE · **NEW** · audit+swarm×4+pr-aggregate **SUCCESS** |
| macOS **#110** | OPEN · CLEAR banked @ `a60ecac` · tip UNCHANGED |
| macOS master / develop | **`da8f0fc`** / **`a60ecac`** |
| macOS **#112** / **#99** | **MERGED** / **CLOSED** SUPERSEDE |
| Win | open **#33 HOLD only** @ `d12321d` · develop `67ec265` · master `f0c2f5a` |
| Linux **#59** / **#58** | OPEN · CLEAR banked @ `b662494` / `387a8ee` · tips UNCHANGED |
| umbrella **#11** | OPEN · HARD AMEND banked @ `d141f26` |
| community / tank | open **zero** · tank main **`85ce800`** |
| CI | Actions dispatching on macos; re-arm still Pete word |

---

## Independent ground

Worktrees: `/tmp/hiwave-pr117-r1` @ `f31e42f` · `/tmp/hiwave-pr117-master` @ `da8f0fc`  
Merge-base ≡ master tip (`da8f0fc`). Tip is **descendant of master**.  
**Engine / crates paths in diff: none.** Scope instrument-only (+2180/−34).

| Path | Role |
|------|------|
| `scripts/wpt_tier1.py` | **UNTOUCHED** (byte-identical master↔tip) |
| `scripts/wpt_seed_scout.py` | **NEW** — restored from #99 salvage; pin-aware reftest scout |
| `scripts/wpt_seed_merge_1a.py` | **NEW** — content-blind round-robin fill to `seed_cap` |
| `trench/wpt/MANIFEST.json` | seed_n **14→30** · seed_cap 30 · pin unchanged |
| `trench/wpt/seed-scout-1A.json` | regenerated census receipt @ pin `a6f29b0` |
| `trench/wpt/last-run.json` | first honest 30-case reading |

### Why #99 SUPERSEDE does not apply here

| #99 defect (banked SUPERSEDE @ `69a5ac2`) | #117 measured |
|------------------------------------------|---------------|
| Tip rewrote harness; blank gate absent | `wpt_tier1.py` **byte-identical** to master |
| Negative control absent | **PRESERVED** (honesty.negative_control on last-run) |
| Blank PASS regression (align-items ERROR→PASS) | old-14 verdict **ERROR blank STABLE** |
| Merge-base pre-#74 (parallel recovery) | merge-base ≡ **post-#112 master** |
| seed expand on broken harness | seed expand **only** on merged harness |

### Harness invariants (must keep)

| Check | Result |
|-------|--------|
| Blank frame → ERROR (not match) | **PRESERVED** (runner identical) |
| Negative control FAIL-as-required | **PRESERVED** (last-run honesty) |
| `WPT_MAX_DIFF_PCT = 0.0` | **PRESERVED** |
| Unrunnable → SKIP | **PRESERVED** (4 SKIPs stable) |
| Webfont FAIL stay scored + `blocked_by` | **PRESERVED** |
| suspect_passes named | **PRESERVED** (same 3 ids) |

### Seed selection (content-blind)

- Scout re-ran bucket 1A at pin `a6f29b0`: **138 CANDIDATE / 18 UNRUNNABLE / 9 NOT-REFTEST** (165 proposals) — matches body.
- Merge fills `seed_cap - len(entries)` via **round-robin across 3 dirs, alphabetical within dir**, blind to render outcomes — cannot cherry-pick passes.
- All **14 master entries preserved** with path/ref/kind/tier/maps_to **identical**.
- **16 new** 1A entries: line-break-anywhere / overflow-wrap-anywhere / break-boundary / word-break-break-all slice.

### Old-14 verdict stability (master → tip)

All 14 cases: status + `diff_pct` **bit-stable**, including:

| Case | Status | diff |
|------|--------|------|
| overflow-wrap-001/002 | FAIL | 0.7054 / 2.495 |
| empty-span-size-002 | FAIL | 0.6658 (honest unattributed head) |
| empty-span-height/scroll/size-001 + empty-text-node | SKIP | reftest-wait / JS |
| align-items-baseline-overflow-non-visible | ERROR | blank frame |

### First honest 30-case reading

| Field | Master (n=14) | Tip (n=30) |
|-------|---------------|------------|
| pass / fail / skip / error | 6 / 3 / 4 / 1 | **7 / 18 / 4 / 1** |
| scored (pass+fail) | 9 | **25** |
| rate | 0.6667 (6/9) | **0.28 (7/25)** |
| runner | `scripts/wpt_tier1.py` | same |
| wpt_pin | `a6f29b0…` | same |
| hiwave_git_sha | 427390c (pre-promote) | **da8f0fc** (post-#112) |

- New cases: **15 FAIL + 1 PASS** (`word-break/break-boundary-2-chars-002`) — body "15 of 16 fail" **CONFIRMED**.
- FAIL attribution: **14** `@font-face` blocked + **4** unblocked (2 line-break-anywhere + 2 break-boundary-related without font tag).
- suspect_passes still: `br-font-size`, `br-line-height`, `overflow-wrap-004`.

### Rate honesty

| Claim | Ruling |
|-------|--------|
| Quote **7/25 (28%)** as engine progress | **HARD NO** — first honest 30-case baseline; rate *drop* vs 6/9 is correct instrument, not regression |
| Compare to #99's 8/26 | **HARD NO** — that runner lacked SKIP gate + negctl |
| Pass count 6→7 | **one** new green only (`break-boundary-2-chars-002`); not a product win banner |
| 14 @font-face fails | one capability gap, not 14 independent text bugs — attribution **CLEAR** |

### CI

audit + pr-swarm ×4 + pr-aggregate: **SUCCESS** (Actions dispatching on macos).

---

## Rulings

| Item | Ruling |
|------|--------|
| #117 seed salvage | **DESIGN CLEAR / APPROVE merge** @ `f31e42f` |
| Master harness authority | **STANDS** (untouched) |
| #99 SUPERSEDE | **unchanged** (closed; salvage path is this PR) |
| Expand font implement / engine | **NO** this PR |
| Quote 7/25 as product win | **HARD NO** |
| Banner seed-cap reached = finish line | **NO** — more CANDIDATEs remain (122 unseeded after this fill) |
| Merge | **Atlas** — not Prometheus |

---

## Seat asks

**Atlas**
1. Land #117 when green (instrument-only; no engine).
2. Do not banner 7/25 without denominator + attribution gloss.
3. Next product residual from this seed: unblocked soft-wrap fails and/or real `@font-face` unit; empty-span-size-002 still honest head among old set.
4. #110 promote remains separate (CLEAR banked; Pete go).

**Argos (optional)**
- Grep: `wpt_tier1.py` absent from diff; blank gate + negative_control still present; seed_n=30; no crates/.

**Pete**
- CI re-arm word still yours; #110 master go separate.

---

## No irreversible

No merge / force-push / spend / master write / null attend from this seat.

**Durable:** this file + WORK_QUEUE tick entry + exchange doorbell-note.
**Prometheus next:** outside-eye first *new* tip after #117 lands or tip moves. Else **STOP** (do not re-pin #117 CLEAR · #110 CLEAR · #112 MERGED · #99 CLOSED · #59/#58 CLEAR · #33 HOLD · #11 HARD AMEND · community/tank zero unless measurement changes).
