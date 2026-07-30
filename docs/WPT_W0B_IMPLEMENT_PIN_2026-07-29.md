# WPT Phase 0.5 — W0b IMPLEMENT pin (first honest K/N)

**Seat:** Prometheus (design only)  
**Date:** 2026-07-29 (grind tick)  
**Status:** **IMPLEMENT_NOW** — W0a is on `master`; no W0b PR open yet.  
**Exists in service of:** PLAN.md north star — absolute WPT Tier-1 conformance, not “matches Chrome’s bug.”  
**Audience:** Atlas / Athena (execute pathfinder first), Pollux (execute-count when PR opens), Pete (Friday trendline only), Prometheus (outside-eye on the PR).

**Companions (measured this tick):**

| Artifact | Location | State |
|----------|----------|--------|
| W0a seed | `master` merge `2ebfc6e` (#69) | **SHIPPED** |
| Manifest SoT | `trench/wpt/MANIFEST.json` | seed_n=14, pin `a6f29b0bedaf…` |
| Sync projector | `scripts/wpt_sync.sh` | **network path never run** (task-0) |
| W0a outside-eye | `docs/WPT_W0A_PR69_REVIEW_2026-07-29.md` | DESIGN CLEAR |
| Phase 0.5 GATE OPEN | hub `hiwave/trench/forensics/2026-07-15-wpt-phase05-GATE-OPEN.md` | **absent from hiwave-macos tree** (R1 below) |
| Campaign path | `target/release/parity-capture` + `scripts/parity_lib.py` | real engine → PPM |

**Not this unit:** #71 CRLF merge (R1 GREEN @ cb730d0 — Atlas lane) · #68 GPU pin (ACCEPTED) · Windows #33 C2 HOLD · GPU bucket-(b) code · CI WPT floor · Tier-2 · second engine host.

---

## 0. One-liner

**W0b = task-0 real sync + thin reftest adapter over existing `parity-capture` + first honest `last-run.json`.**  
Path **P0 stands.** Do not revive `rustkit-test` HTML-strcmp as conformance. Do not invent a second headless host.

---

## 1. Why this unit now

| Fact | Measurement |
|------|-------------|
| Campaign meter saturated | W0a README / #69 body: **26/26 @ t15** — cannot distinguish improve vs plateau |
| W0a on master | `2ebfc6e` — list + `wpt_sync` + honesty docs; **no** K/N |
| Open design residual on WPT lane | **W0b unopened** — highest bankable design unit after #69 CLEAR and #71 R1 filled |
| Live open PRs needing *new* design | #71 already R1+R2 · #68 decisions already LOCKED · Windows #43 Pollux R1 · #33 HOLD — none re-pin |

Queue rule (standing): after W0a CLEAR → first *new* residual **or W0b design**. PR-empty for W0b → bank **IMPLEMENT pin** so the first execution night is not improvisation.

---

## 2. Doctrine re-ratified (do not re-litigate)

From GATE-OPEN §3–5 + W0a review §2 — still fleet law:

1. **Manifest is SoT.** `third_party/wpt/` is a gitignored projection of `MANIFEST.json`. Never reverse-drive the list from a checkout. Never commit the WPT tree.
2. **Engine pixels only.** Test and ref both render through **the same** `parity-capture` binary the campaign uses. HTML-normalize / rustkit-test reftest = instrument lie.
3. **`rel=match` is authority.** Manifest `ref` is a listing candidate. Disagreement with the test file’s `<link rel=match>` = **instrument error**, not render fail.
4. **Skip ≠ fail.** Missing ref, unsupported `@supports`, JS dependence → `SKIP` + reason.
5. **No PR CI gate** until Pete locks a floor. W0b may write `last-run.json` and optional nightly/Friday artifact only.
6. **Honesty over green.** An all-green first W0b run with N=14 is **presumptively a lying harness** (same family as empty-capture 100% / “Worst 3” that named best cases). Exit needs evidence the oracle can go red.
7. **Campaign meter stays.** Registry / CfT-148 t15 untouched. WPT is additive.
8. **macOS pathfinder first.** Windows/Linux twin after macOS W0b green (or parallel only if idle + same MANIFEST/pin).

---

## 3. Implement stack (ordered — one PR preferred)

### Task-0 — first real `wpt_sync.sh` (budget breakage)

```bash
./scripts/wpt_sync.sh            # network sparse-checkout at MANIFEST pin
./scripts/wpt_sync.sh --check    # must exit 0: 28/28 paths on disk
```

| Rule | Detail |
|------|--------|
| Expect fixes | Allowlist / sparse-cone / depth-1 fetch — **do not pretend green** on author seat that never ran network path |
| Exit task-0 | `--check` OK; every MANIFEST path + ref present under `third_party/wpt/` |
| Non-goal | Growing seed past 14 in the same PR unless tree checkout unblocks **one** listing-proven pair without guesswork |

### Task-1 — `rel=match` binding pass (instrument, not pixels)

For each MANIFEST entry:

1. Parse test HTML for `<link rel="match" href="…">` (and `mismatch` if present — **do not auto-run mismatch as pass** in W0b).
2. Resolve href relative to test path.
3. If resolved path ≠ MANIFEST `ref` → record **`INSTRUMENT`** (or `ERROR`) with both paths; **do not** pixel-diff.
4. Optional: rewrite MANIFEST `ref` only with explicit commit note; default is report + skip pixels.

### Task-2 — runner CLI (`scripts/wpt_tier1.py` recommended name)

**Input:** `trench/wpt/MANIFEST.json`  
**Binary:** `target/release/parity-capture` (build once; refuse to invent another capture path).

Per entry (`kind: reftest`):

| Step | Contract |
|------|----------|
| Viewport | **`MANIFEST.default_viewport` = 800×600** unless test meta / future per-entry override. **Do not silently use parity-capture CLI defaults (1280×800).** Pass `--width 800 --height 600` every call. |
| Capture test | `parity-capture --html-file <test> --width 800 --height 600 --dump-frame <work>/test.ppm` |
| Capture ref | same binary, same viewport → `<work>/ref.ppm` |
| Blank gate | Reuse `analyze_frame_blankness` discipline from `parity_lib` (or equivalent): blank/refuse ≠ “100% match” |
| Pixel compare | **test.ppm vs ref.ppm** (both HiWave). Not test vs Chrome. Reuse PPM reader + pixel loop; do **not** call the Chrome-baseline Node oracle for W0b. |
| Threshold | Default **exact RGB match** for W0b seed (WPT reftest semantics). If AA forces noise, document a **single** `wpt_max_diff_pct` (start ≤ **0.1%** or exact) in runner + last-run — **no silent campaign t15 borrow**. Pete only if raising the floor. |
| Classification | `PASS` / `FAIL` / `SKIP` / `ERROR` (instrument) |

**Hard refuse:**

- Using `rustkit-test` reftest / HTML strcmp  
- Publishing campaign t15 as “WPT %”  
- Lowering thresholds to force a pretty first rate  
- Adding PR-merge workflow that fails on WPT  

### Task-3 — `trench/wpt/last-run.json` (the only quotable K/N)

Minimum schema:

```json
{
  "schema": 1,
  "wpt_pin": "<from MANIFEST>",
  "hiwave_git_sha": "<pathfinder HEAD>",
  "runner": "scripts/wpt_tier1.py",
  "viewport": { "width": 800, "height": 600 },
  "ts": "<UTC ISO>",
  "n": 14,
  "pass": 0,
  "fail": 0,
  "skip": 0,
  "error": 0,
  "rate": null,
  "cases": [
    {
      "id": "…",
      "status": "PASS|FAIL|SKIP|ERROR",
      "reason": null,
      "diff_pct": null,
      "diff_pixels": null,
      "rel_match": "ok|mismatch|missing",
      "maps_to": "slice-0"
    }
  ],
  "honesty": {
    "all_green_suspect": true,
    "note": "First run with pass==n and fail==0 and error==0 is presumptively a lying harness unless N is tiny exploratory — W0b exit forbids that shape for full seed."
  }
}
```

| Field rule | Detail |
|------------|--------|
| `rate` | `pass / (pass+fail)` only; **exclude** skip+error from denominator; `null` if pass+fail==0 |
| Quoting | Anyone quoting a WPT % **without** this file (or with rustkit-test) is quoting nothing — README already says so; keep it |
| Commit vs artifact | Prefer **commit** `last-run.json` on the W0b PR so Fridays have a tree-local receipt; regenerate on master after merge OK |

### Task-4 — honesty / exit gates (PR must show)

| Gate | Requirement |
|------|-------------|
| **E0** | Task-0 `--check` green in PR description or CI log (not PR merge-gate) |
| **E1** | At least one case is **not** PASS (FAIL or SKIP or ERROR) on the full seed **or** a checked-in **deliberate negative control** (e.g. force-diff fixture) proves the oracle can go red |
| **E2** | Zero cases scored PASS via HTML strcmp / missing frame / blank-as-match |
| **E3** | Campaign parity workflows **unchanged** (no WPT job required; if added, `continue-on-error` or non-required until Pete floor) |
| **E4** | Runtime for seed_n=14 **&lt; 5 min** local (GATE-OPEN contract) |

**Exit W0b (definition of done):** `Tier-1 pass = K/N` printable from `last-run.json`; pin + hiwave SHA recorded; E0–E4 hold.

---

## 4. Out of scope for this PR (bank, do not fold)

| Item | When |
|------|------|
| **W0c** digest row `WPT Tier-1: K/N @ pin` | After first last-run on master |
| Seed growth toward 25–32 | Separate PR; only with tree checked out + both files verified |
| Per-test viewport meta / reftest.list runner | Later |
| `rel=mismatch` suite | Later |
| Windows / Linux twin runner | After macOS W0b green |
| Pathfinder capability checks / GPU (b) | Orthogonal (#68 pin) |
| C2 net-cache security | HARD HOLD remains |

---

## 5. Residuals carried forward

| ID | Item | Owner |
|----|------|--------|
| **R1** | GATE-OPEN.md linked from WPT README/MANIFEST but **missing** on hiwave-macos tree; canonical copy lives on hub `hiwave/trench/forensics/2026-07-15-wpt-phase05-GATE-OPEN.md` | Atlas: copy or soft-fix into `trench/forensics/` (docs-only OK; may ride W0b or tiny follow-up) |
| **R2** | Hub-only `LINE_BOX_WPT_ROADMAP.md` / `WPT_TIER1_SUBSET.md` links | Same docs hygiene; non-blocking for runner |
| **R3** | Accidental `.pyc` untrack | Prior #65 allowlist chore if still live |
| **R4** | #71 merge | Atlas — R1 GREEN on HEAD `cb730d0` already posted; not W0b |

---

## 6. Outside-eye checklist (Prometheus when W0b PR opens)

Copy of GATE-OPEN §7 W0b + this pin:

- [ ] Renders through **engine** (`parity-capture`), not HTML strcmp  
- [ ] Viewport **800×600** from MANIFEST (not silent 1280×800)  
- [ ] `rel=match` checked; mismatch → ERROR/INSTRUMENT, not FAIL-as-paint  
- [ ] `last-run.json` has pin + hiwave SHA + N + statuses; rate arithmetic honest  
- [ ] E1 honesty (not all-green-without-proof)  
- [ ] Campaign CI gates **unchanged** / no hard WPT PR floor  
- [ ] No full-tree WPT vendor  

**Reject:** full WPT import · campaign-as-WPT · threshold cosplay · folding engine features into the runner PR.

---

## 7. Seat actions

| Seat | Action |
|------|--------|
| **Atlas** | Open W0b PR from this pin (pathfinder). Task-0 first. Merge #71 on own lane when ready (independent). Optional R1 copy of GATE-OPEN into tree. |
| **Athena** | May take W0b if Atlas routes trench night; **same** MANIFEST/pin/viewport. Do **not** start Windows twin until macOS last-run exists. Rail C1 / Length work remains unblocked and orthogonal. |
| **Pollux** | On W0b PR: execute-count — named commands, pass/fail/skip tallies, confirm binary path. |
| **Talos / Argos** | No Linux WPT mirror required for W0b. Linux PR queue order / R1 lane unchanged. |
| **Pete** | None on design. Optional later: lock “no WPT floor in PR CI until N≥50 and two stable Fridays.” |
| **Prometheus** | Outside-eye when PR opens; no re-pin of this implement doc unless measurement contradicts (viewport default, capture binary, honesty exit). |

---

## 8. What Prometheus is not doing this tick

- Not implementing the runner or running network `wpt_sync` as if that were design  
- Not merging #71 / #68 / #43  
- Not inventing a WPT CI floor  
- Not re-pinning Gradient DEFER, C2 HARD HOLD, Length method, GPU (b) decisions, or W0a CLEAR  
- Not opening W0c until first last-run lands  

---

## 9. One-line summary

**W0b is ready to build: real sync → same `parity-capture` for test and ref at 800×600 → honest `last-run.json` that can go red — IMPLEMENT_NOW, path P0 only.**
