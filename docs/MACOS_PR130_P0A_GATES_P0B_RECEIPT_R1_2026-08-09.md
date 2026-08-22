# Outside-eye R1 — hiwave-macos PR #130 (P0a four gates + P0b 1/26 receipt)

**Seat:** Prometheus (design only)  
**Date:** 2026-08-09  
**Tip:** `e2dba9c` on `atlas/trench-parity-finish-line`  
**Base:** `master` @ `44389f1`  
**PR:** https://github.com/hiwavebrowser/hiwave-macos/pull/130  
**Verdict:** **DESIGN CLEAR / APPROVE merge** @ `e2dba9c`

---

## Board re-measure (this tick)

| Surface | Tip / state |
|---------|-------------|
| macOS **#130** | tip **`e2dba9c`** · OPEN · MERGEABLE · **NEW residual past idle pin `11f4a35`** · audit+swarm×4+aggregate+selector-key **SUCCESS** |
| macOS **#110** | tip **`9c30630`** · OPEN · residual past CLEAR `7f59b35` = **#133 only** (CLEAR banked + MERGED) · not re-pinned |
| macOS **#133** | **MERGED** → develop @ `9c30630` (CLEAR banked @ `4c1fc44`; ResourceType cfg soft residual closed @ `57efc18`) |
| macOS master / develop | **`44389f1`** / **`9c30630`** |
| Win | open **#33 HOLD only** @ `d12321d` · develop **`36c3b75`** · #88 human-keyboard receipt **FALLEN** |
| Linux **#59** | OPEN · CLEAR body banked · tip `7ad1eb0` |
| community **#6** | OPEN · CLEAR banked @ `f6b7891` |
| tank | open **zero** |
| umbrella **#11** | OPEN · HARD AMEND banked @ `0b5993d` |

**Queue rule held:** banked CLEARs stay banked. Next was outside-eye first *new* tip. #130 tip moved past the prior "instrument idle" deferral with P0b conjunction + CI green → this unit.

---

## Independent ground

Worktree: `/tmp/hiwave-pr130-r1` @ `e2dba9c`. Master ref: `origin/master` @ `44389f1`.

### Scope

| Unit | Result |
|------|--------|
| Files | **22** · scripts + tools + trench + `.github/workflows/parity.yml` only |
| Diff | **+7714 / −230** |
| `crates/` · `Cargo.toml` · `Cargo.lock` | **byte-identical** to master (0 bytes of diff) |
| `*.rs` | **zero** |
| Engine behavior | **zero changes** — P0b number is attributable to instrument + master's engine |
| merge-tree write-tree vs master | **CLEAN** (`94fe7948…`) |

### Residual past prior idle pin `11f4a35`

| Commit | Role |
|--------|------|
| `c9b2b5e` | `finish_line_receipt.py` — four-way conjunction N/26 |
| `6a22ea3` | CI: guard suite lane guards unrunnable → fixed |
| `b34aff5` | P0b — first N/26 receipt claimed 1/26 on macOS |
| `344268e` | delete duplicated lane guard that needed a parser |
| `e2dba9c` | docs: trench session overlap note |
| (+ body through `11f4a35`) | Gates A/B/C + stability + sharding + selector-key |

### Gate design census

| Gate | File | Bar | Exit / CI posture | Result |
|------|------|-----|-------------------|--------|
| **A** geometry | `layout_oracle_gate.py` | ≤0.5px · join on selector · registry viewport only · phantom/ambiguous fail closed | advisory (`continue-on-error: true`) · `if: always()` | **CLEAR** — stub gone; `GEOMETRY_TOLERANCE_PX=0.5` single constant; off-viewport → UNMEASURED not wrong-measure |
| **B** paint | `paint_oracle_gate.py` | ≥99% within policy `aa_tolerance: 5` + discrete structural auto-fails | advisory | **CLEAR** — tolerance read from `VISUAL_DIFF_POLICY.md`; discrete separate from percentage |
| **C** forensic | `forensic_board.py` | publish board; never number-gate | non-gating; **exit 1** if policy missing or 0 cases measured | **CLEAR** — load-bearing did-not-run path in `main()` |
| stability | `parity_gate.py` + swarm | `STABILITY_MIN_RUNS=3` measured iterations at `pr_merge` / nightly | **blocking** (not waived) | **CLEAR** — evidence not exemption; old single-run waiver removed |
| N/26 receipt | `finish_line_receipt.py` | geometry ∧ paint ∧ stability ∧ discrete; unmeasured never green | non-gating; exit 1 if receipt measured nothing | **CLEAR** — `conjoin()` all four measured AND green |
| join key | `verify_selector_key.mjs` + comment pin on `capture_baseline.mjs` | reproduce committed selectors; extract function, do not copy | CI job SUCCESS | **CLEAR** — capture change is comments only; `/\\s+/` accidental contract named, not tidied |

### Sharding (load-bearing)

Prior defect CONFIRMED in prose+code: unit-index shard scattered a cell's iterations across shards so no cell could show 3 measured runs → stability always unmeasurable under swarm. Tip shards by **(case, viewport) cell**, keeping all iterations of a cell on one shard. **CLEAR**.

### Local verification (this seat)

```
python3 -m pytest scripts/tests/test_{layout_oracle_gate,paint_oracle_gate,forensic_board,finish_line_receipt,stability_actually_gates,sharding_preserves_stability_evidence,parity_image}.py -q
→ 144 passed in ~202s
```

CI @ tip: audit · script-guards · selector-key · pr-swarm ×4 · pr-aggregate **SUCCESS**.

### P0b `1/26` claim (honesty)

| Claim | Independent status |
|-------|-------------------|
| Conjunction script exists and mutation-tested | **CLEAR** (local suite) |
| Zero engine delta vs master | **CLEAR** (byte-identical crates) |
| Receipt measured on macOS-14 CoreText+Metal | **ACCEPT stated** via baseline+digest+CI lane — Prometheus did **not** re-swarm 26 cases this tick |
| `bg-pure` only finish-line green | **ACCEPT stated** (campaign document; not re-measured) |
| Quote 1/26 as *engine* progress | **HARD NO** — same engine as master; number is instrument truth |

### Soft residual (non-blocking)

**CI swallow of Gate C / finish-line exit 1:** both steps use `continue-on-error: true`. Script exits 1 on did-not-run (correct), but the *workflow* stays green. Visibility relies on step outcome + job-summary `emit_receipt`. Acceptable for the ratified **advisory-first** cycle; when A/B flip to blocking, drop `continue-on-error` on A/B only — keep C/finish-line non-gating *or* promote "did-not-run" to a hard workflow fail without number-gating. **SOFT** — do not expand this PR.

### #88 relationship

#88 (trench baseline SUPERSEDE banked) is the stale pre-instrument docs path. **#130 is the real P0a/P0b instrument.** Close or HARD AMEND #88 if still open under another number; do not land stale baseline prose over this tip.

---

## Rulings

| Item | Ruling |
|------|--------|
| #130 product (instrument + receipt plumbing) | **DESIGN CLEAR / APPROVE** @ `e2dba9c` |
| Zero engine behavior | **HARD KEEP** |
| P0b 1/26 as measurement of current master engine | **ACCEPT / bank as campaign baseline** |
| Quote 1/26 as engine win / parity victory | **HARD NO** |
| Flip A/B to blocking this PR | **HARD NO** — Pete/open decision; wait for deliberate macOS cycle |
| Tidy `getSelector` `/\\s+/` without regen | **HARD NO** |
| Expand to engine paint/geometry fixes | **NO** this PR (P0c+ campaign) |
| Merge | **Atlas** — not Prometheus |
| #110 residual @ `9c30630` | **no new product residual** past banked #133 CLEAR — do not re-pin |

---

## Actions by seat

- **Atlas:** land #130 when green; optional soft note on continue-on-error vs did-not-run visibility; do not banner 1/26 as product progress; next = real engine work against the honest 1/26 (geometry join failures ~115 named in digest) under separate PRs.
- **Argos:** optional tip re-R1 greps (zero crates · STABILITY_MIN_RUNS · conjoin · board_ran exit 1).
- **Pete:** when ready — A/B advisory→blocking decision (prefer after a second clean macOS receipt cycle); master go is separate from #110.
- **Prometheus next:** outside-eye first *new* tip only. Else **STOP** (do not re-pin #130 CLEAR @ `e2dba9c` · #133 MERGED · #110 CLEAR through #133 residual · flip HARD NO · #33 HOLD · #59 CLEAR body · #11 HARD AMEND · community #6 CLEAR · tank zero · Win keyboard FALLEN unless measurement changes).

---

## What this seat did not do

No merge, force-push, spend, master write, null attend, branch delete, or engine edit.
