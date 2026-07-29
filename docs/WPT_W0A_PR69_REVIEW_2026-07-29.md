# Outside-eye: hiwave-macos PR #69 — Phase 0.5 W0a (WPT Tier-1 seed)

**Seat:** Prometheus (design only)  
**Date:** 2026-07-29  
**PR:** https://github.com/hiwavebrowser/hiwave-macos/pull/69  
**Tip reviewed:** `2d48a7d` on `atlas/wpt-w0a`  
**Base:** `master` @ `795768a`  
**Verdict:** **DESIGN CLEAR · APPROVE merge** (Atlas/Pete merge lane; this seat does not merge)

---

## 0. Why this unit

Campaign metric is saturated: PR body + README re-measure **26/26 @ t15, avg 6.7%** on committed master with fresh `parity-capture`. A pinned-Chrome meter at 26/26 cannot distinguish improvement from plateau. PLAN north star is absolute conformance; W0a is the first brick (list only — no K/N).

Queue rule for this grind tick: highest *new* open PR needing design after Length/#42 method pin and prior CLEAR pins. #69 is that residual. #68 GPU pin is docs-only and already scoped as non-blocker; Length #42 method already pinned exchange #332.

---

## 1. Scoreboard (measured, not recalled)

| Claim | Measurement | Ruling |
|-------|-------------|--------|
| Scope = list + tooling + instrument fix; no engine; no CI job; no gate | Diffstat: 7 files, +284/−8; no workflow; no crate behaviour change beyond docs on reftest | **PASS** |
| `seed_n` ≤ 30 | MANIFEST: `seed_n=14`, `seed_cap=30`, 14 entries / 14 unique paths / 14 unique refs | **PASS** |
| Every manifest path resolves at declared pin | Independent HTTP GET vs `web-platform-tests/wpt@a6f29b0…` — **28/28 = 200** (14 tests + 14 refs) | **PASS** |
| Pin SHA real | GitHub commit `a6f29b0bedaf` exists (2026-07-29); message WebTransport SendGroup (unrelated; pin is freeze point, not topical) | **PASS** |
| Sync does not vendor WPT into monorepo | `scripts/wpt_sync.sh` projects manifest → gitignored `third_party/wpt/`; `.gitignore` adds that path | **PASS** |
| Stub reftest honesty | Module-doc on `rustkit-test` reftest: no parse/style/layout/paint; must not quote as conformance | **PASS** |
| No published WPT pass-rate | No `last-run.json`; README + MANIFEST `_comment` forbid quoting % before it exists | **PASS** |
| Banner "Worst 3" actually worst | `worst_first`: unmeasured first, then DESC diff; 4 unit tests incl. live-board regression | **PASS** |
| CI green | `gh pr checks` / statusCheckRollup: pr-swarm 0–3 + pr-aggregate + collect-metrics **SUCCESS** | **PASS** |
| GATE-OPEN.md linked from README/reftest/MANIFEST | Path `trench/forensics/2026-07-15-wpt-phase05-GATE-OPEN.md` **not present** on `origin/master` or this branch | **RESIDUAL R1** (docs integrity; non-blocking for W0a content) |

---

## 2. Design pin ratification (W0a)

**Ratify as fleet doctrine for the WPT lane (macOS pathfinder first):**

1. **Manifest is source of truth.** Working tree under `third_party/wpt/` is a projection; never commit the WPT tree; never reverse-drive the list from a checkout.
2. **W0a ships the list; W0b ships the runner + first honest K/N.** No pass-rate without `trench/wpt/last-run.json`.
3. **`rel=match` is authority.** Manifest `ref` is a same-directory/listing candidate. Runner treats disagreement as **instrument error**, not render fail.
4. **No CI merge gate** until a floor is deliberately locked (standing Pete rule + pin §5 class). W0a correctly adds zero workflows.
5. **Honesty over green.** An all-green first W0b run is presumptively a lying harness (same family as empty-capture 100% / banner that named best cases "worst").
6. **Campaign meter stays.** Registry/CfT-148 parity is untouched; WPT is additive.

---

## 3. What is deliberately open (accepted)

| Gap | Accept? | Notes |
|-----|---------|--------|
| `wpt_sync.sh` network path unrun on author seat | **Yes** | `--dry-run` / `--check` exercised; first real sync is W0b task-0 |
| Seed 14 not 25–32 | **Yes** | Only pairs with both files visible in pinned listing; grow with tree checked out |
| Test→ref bindings unverified | **Yes** | Correct instrument hierarchy |
| Tracked `.pyc` from #65 still tracked | **Yes / ledger** | New ignore stops new ones; untrack is Atlas allowlist (`git rm --cached`) |
| GATE-OPEN.md missing from tree | **R1** | Recover or replace with this PR's README as local pin; do not invent history |

---

## 4. Residuals (non-blocking for merge)

| ID | Item | Owner |
|----|------|--------|
| **R1** | Broken links to `trench/forensics/2026-07-15-wpt-phase05-GATE-OPEN.md` (and hub refs `LINE_BOX_WPT_ROADMAP.md` / `WPT_TIER1_SUBSET.md` if not in-tree) | Atlas follow-up docs PR, or recover file if it lived only on a seat disk |
| **R2** | First real `wpt_sync.sh` run will likely need a fix — budget W0b task-0, do not pretend green | Athena/Atlas trench |
| **R3** | W0b design (when opened): same `parity-capture` path as campaign; skip-with-reason ≠ fail; pin test that all-green first run is treated as suspect | Prometheus outside-eye when PR opens |
| **R4** | Untrack accidental #65 `.pyc` when allowlist permits | Atlas |

No R-item blocks W0a merge.

---

## 5. Seat actions

| Seat | Action |
|------|--------|
| **Atlas** | Merge #69 when ready under existing hiwave-macos policy. Optional same-day or follow-up: R1 recover/soft-fix GATE link; R4 untrack pyc. Do not add CI gate in this PR. |
| **Athena / trench** | After merge: W0b = sync + reftest adapter over **existing** capture/pixel path (path P0). Do not build a second engine host. Report first `last-run.json` with skips explained. |
| **Talos / Pollux** | No Linux/Windows WPT mirror required for W0a. Method transfers later; seed list is macOS pathfinder. |
| **Pete** | None on design. Optional: confirm WPT still must not red-lock PR merges until a floor is locked (already standing). |
| **Prometheus** | No re-review #69 unless tip scope expands (CI gate added, false pass-rate published, engine change folded in). Next: first *new* design residual or W0b PR outside-eye. |

---

## 6. What Prometheus is not doing

- Not merging #69
- Not running `wpt_sync` network path as if that were design
- Not inventing a WPT CI floor
- Not re-pinning Length #42 method, #65 CLEAR, Gradient DEFER, or C2 HARD HOLD
- Not scoring campaign 26/26 independently this tick (accepted as author re-measure on `795768a`; W0a does not depend on re-proving the board)

---

## 7. One-line summary

**W0a is the right brick at the right time: honest seed list, no false %, no CI lock, instrument banner fixed — DESIGN CLEAR to merge.**
