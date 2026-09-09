# Outside-eye R1 — hiwave-macos PR #130 tip residual `5d98c4e`

**Date:** 2026-08-11  
**Seat:** Prometheus (Grok / design, scheduled grind)  
**Scope:** tip moved past banked product residual CLEAR `6b09f9d`  
**PR:** [hiwavebrowser/hiwave-macos#130](https://github.com/hiwavebrowser/hiwave-macos/pull/130)  
**Branch:** `atlas/trench-parity-finish-line`  
**Base:** `master` @ `44389f1`  
**Tip:** `5d98c4ef6b053ffa4595df4c05895751c791a8d9` · OPEN · MERGEABLE  
**CI (this tip):** audit + pr-swarm×4 + pr-aggregate + script-guards + selector-key **SUCCESS**

---

## Queue rule

Banked CLEARs stay banked. Next = outside-eye first *new* tip.  
**#130 tip MOVED** past banked product residual `6b09f9d` → `5d98c4e` (4 commits).

### Live board (this tick)

| Surface | Tip / state |
|---------|-------------|
| macOS **#130** | tip **`5d98c4e`** · OPEN · MERGEABLE · **NEW residual** |
| macOS **#110** | tip **`9c30630`** · OPEN · CLEAR banked (not re-pinned) |
| macOS master / develop | **`44389f1`** / **`9c30630`** |
| Win | open **#33 HOLD only** @ `d12321d` · develop **`36c3b75`** · keyboard FALLEN |
| Linux **#59** | tip **`7ad1eb0`** · CLEAR body · tip UNCHANGED |
| community **#6** | OPEN · CLEAR banked @ `f6b7891` |
| tank / umbrella | open **zero** / #11 HARD AMEND banked @ `0b5993d` |
| P1 engine PR | **NONE open** — residual still stacked on #130 tip past `e2dba9c` |

### Banked pins that STAND (not re-litigated)

| Pin | SHA / claim |
|-----|-------------|
| Instrument CLEAR | `@ e2dba9c` · crates ≡ master · P0a+P0b |
| Product residual CLEAR (P1 overflow rounded clip) | `@ 6b09f9d` · engine unit approved as *separate* product |
| Packaging | **SPLIT** — restore instrument-only @ e2dba9c; open P1 engine PR vs develop |
| Land tip as-one / land-as-instrument at 6b09f9d+ | **HARD NO** |
| Quote 1/26 as engine win | **HARD NO** |
| Flip / horizontal subpixel | HARD NO / FALSIFIED @ `7863e81` |

---

## Residual under review

### Commits `6b09f9d..5d98c4e` (4)

| SHA | Summary |
|-----|---------|
| `eb12d55` | **fix(parity):** Gate B discrete detectors were reporting Gate A's defects |
| `9679857` | **test(parity):** close mutation-sweep gap — geometry precondition on *both* detectors |
| `5d30c0a` | trench: discrete column was measuring Gate A; 62/62 proved it |
| `5d98c4e` | trench: retract night 7 discrete headline (evidence on withheld boxes) |

### Diff shape

| Path | Δ |
|------|---|
| `scripts/paint_oracle_gate.py` | +~150 instrument |
| `scripts/tests/test_paint_oracle_gate.py` | +~244 tests |
| `trench/BASELINE-parity-finish-line.md` | +1 |
| `trench/digest-parity-finish-line.md` | +~170 digest |
| **`crates/`** | **empty vs `6b09f9d`** |

**This residual is instrument honesty only.** Zero engine behavior change past the already-banked product residual.

### Packaging still breached at tip

Tip vs master still carries P1 engine crates:

```
crates/rustkit-engine/src/lib.rs
crates/rustkit-layout/src/lib.rs
crates/rustkit-renderer/src/lib.rs
```

(`e2dba9c..5d98c4e` crates: +878/−69 — same engine residual banked at 6b09f9d.)  
**SPLIT pin is not executed.** Tip is still "instrument title + stacked engine + new Gate B honesty."

---

## Independent ground

Worktree: `/tmp/hiwave-pr130-r1-tip-5d98c4e` @ `5d98c4e`.

### Defect (instrument)

Both discrete detectors (`detect_missing_clip`, `detect_wrong_solid_color`) sample **RustKit pixels at Chrome's rect**. That is a paint claim **only if** RustKit placed the box where Chrome did.

Author measurement (26-case corpus, same captures, Linux/SwiftShader seat — receipt shape, not macOS SoT for N/26):

| Claim | Result |
|-------|--------|
| `missing_clip` auto-fails on geometrically exact elements | **0** |
| `missing_clip` auto-fails on Gate-A-failing (displaced) elements | **62 of 62** |
| Displacement range cited | 8px–384px |
| Clean example | `css-selectors` `div.section:nth-of-type(3)` reported as unclipped corner while rounded correctly **21px higher** (true box location) |

Module already stated this for `paint_outside_box` and deferred it ("needs RustKit layout dump joined"). **This residual is that unit.**

### Fix

1. **`attributable_selectors(elements, rustkit_layout)`** — admit a selector to discrete detectors only when:
   - join finds **exactly one** RustKit box for the selector
   - border box matches Chrome rect within **Gate A's tolerance on every axis**
2. **Import, never restate** from `layout_oracle_gate`:
   - `AXES = ('x', 'y', 'width', 'height')`
   - `GEOMETRY_TOLERANCE_PX = 0.5`
   - `border_box`, `find_layout_json`, `index_rustkit`
3. Missing / unreadable layout dump → case **UNMEASURED** (`no_rustkit_layout`) → fails (not scored blind).
4. Summary exposes **`discrete_unattributable` / withheld** so silence is visible.
5. Mutation sweep closed: geometry filter on **both** detectors (M6 survived when only clip was filtered).

### Stated limit (ACCEPT)

Exactly-placed element can still have a **displaced sibling** painting into its corner. Needs overlap analysis this gate does not do. Residual closes "element under test is itself somewhere else."

### Local verification (this seat)

| Unit | Result |
|------|--------|
| Import pin | `GEOMETRY_TOLERANCE_PX == 0.5` · `AXES == ('x','y','width','height')` |
| `pytest scripts/tests/test_paint_oracle_gate.py` | **34 passed** (~19m) |
| + `test_layout_oracle_gate.py` | **50 passed** combined |
| Load-bearing T-RED shapes present | admit/withhold at 0.5 vs 0.51 · missing join · ambiguous join · displaced cannot be missing_clip · displaced cannot be wrong_solid_color · withheld counted · no layout → unmeasured |
| merge-tree vs master | CLEAN write-tree (no conflict markers) |
| CI tip | SUCCESS (audit/swarm×4/aggregate/selector-key/script-guards) |

### Metric honesty (author + digest)

| Column | Before residual (night 7 story) | After residual |
|--------|----------------------------------|----------------|
| paint-green | 1/26 | 1/26 (bit-identical percentage half claimed) |
| N/26 | 1/26 (blocked by geometry ∧ …) | **1/26 unchanged** (no case was discrete-only red) |
| discrete auto-fails | 62 (misattributed) → night 7 "51→35" on engine | **0 structural** after filter; examined 172 / withheld 1421 of 1593 |
| Night 7 "51→35 engine proof" | was cited as P1 evidence | **RETRACTED** — deltas were on withheld/displaced boxes |

**N/26 cannot improve from this residual alone.** Conjunction already failed geometry-red cases. What changes is the discrete column stops double-counting Gate A.

---

## Rulings

| Item | Ruling |
|------|--------|
| Residual product (Gate B geometry precondition) | **DESIGN CLEAR / APPROVE** as instrument honesty unit @ `5d98c4e` |
| Prior product CLEAR @ `6b09f9d` (P1 clip) | **STANDS** (no crate delta in residual) |
| Prior instrument CLEAR @ `e2dba9c` | **STANDS** for that SHA only |
| Packaging SPLIT pin (2026-08-10) | **STANDS — still not executed** |
| Merge tip as "instrument only" | **HARD NO** (engine crates still present) |
| Land tip as-one (instrument + P1 + Gate B honesty) | **HARD NO** (same packaging law) |
| Quote night 7 discrete 51→35 as engine/P1 win | **HARD NO / RETRACTED** |
| Quote discrete 62→0 as paint progress | **HARD NO** — reclassification; N/26 flat |
| Quote 1/26 as engine win | **HARD NO** (unchanged) |
| Sibling-overlap limit | **ACCEPT stated** — not this residual |
| Merge | **Atlas (+Pete path)** — not Prometheus |

### Preferred packaging (reaffirm)

1. **E0** — Restore #130 tip to **instrument-only** including this residual's honesty if desired:
   - either rebase Gate B commits (`eb12d55`..`5d98c4e`) onto `e2dba9c` with **crates ≡ master**, or land instrument @ e2dba9c and land Gate B as a thin follow-up instrument PR
2. **E0a** — Open **P1 clip engine PR** vs develop from residual `6c7c6f3..6b09f9d` (crate delta only); Argos R1 on four named residuals
3. Do **not** treat night 7 discrete deltas as P1 proof when pitching E0a

Conditional land tip as-is remains **worse** than SPLIT and is not approved here.

---

## Seat actions

| Seat | Action |
|------|--------|
| **Atlas** | Execute SPLIT. Optionally fold Gate B honesty into instrument line (crates-empty). Do not banner discrete 62→0 or 51→35 as engine progress. |
| **Argos** | Re-smoke instrument when crates ≡ master; R1 P1 engine PR on four residuals when opened. Optional greps: `attributable_selectors`, `no_rustkit_layout`, import of `GEOMETRY_TOLERANCE_PX`. |
| **Pete** | Master gate on instrument #130 when restored; no design re-litigation of SPLIT unless you want land-as-one (Prometheus still pushes back). |
| **Athena / Talos / Pollux** | No action; residual is macOS instrument. |
| **Prometheus** | Pin stands. Next: outside-eye first *new* tip only (restored instrument if drifted · P1 engine PR · other surfaces). Else **STOP**. |

---

## Explicit non-actions (this seat)

- No merge / force-push / master write  
- No null attend  
- No spend  
- No re-pin of banked CLEARs beyond this tip residual  
- No commit/push of this doc (uncommitted macos docs lane for Atlas)

---

## Verdict one-liner

**DESIGN CLEAR** on Gate B geometry-precondition residual @ `5d98c4e` (instrument honesty). **SPLIT packaging pin STANDS.** Night 7 discrete headline **RETRACTED** as engine evidence.

— Prometheus / Grok seat 2026-08-11 · scheduled grind
