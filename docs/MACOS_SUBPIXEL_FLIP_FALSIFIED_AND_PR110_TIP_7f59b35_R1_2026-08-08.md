# Outside-eye R1 — subpixel flip FALSIFIED + #110 tip residual `7f59b35`

**Seat:** Prometheus (design only)  
**Date:** 2026-08-08  
**Unit A — #110 promote residual:** tip `7f59b355cb3a648d9b851fb7e97b9a87cb6d8caa` ≡ `origin/develop`  
**Unit B — flip tip (no PR):** `7863e813152db40f885968ed51146b30b1dfa25a` on `atlas/subpixel-flip`  
**Master at measure:** `44389f1`  
**Verdicts:**

| Unit | Verdict |
|------|---------|
| **#110** @ `7f59b35` | **DESIGN CLEAR / APPROVE** promote residual (banked #131+#132 bodies only; production still phase-0 frozen) |
| **`atlas/subpixel-flip` @ `7863e81`** | **Implementation CLEAR · merge HARD NO** — measured, falsified as major text-residual driver; preserve branch as evidence; do not land without a new reason |

---

## 0. Queue rule

Banked CLEARs stay banked. Next unit = outside-eye first *new* tip.  
Queue named (in order): call-site flip PR · #110 if tip moves past `207a04e` · #130 if design residual · …  

This tick found **both**:

1. **#110 tip moved** `207a04e` → `7f59b35` (+#132 merge + #131 merge — both already CLEAR-banked as independent PRs, now **MERGED**).
2. **Call-site flip tip exists** on `atlas/subpixel-flip` @ `7863e81` — **no open PR** (commit subject: `MEASURED, FALSIFIED, NOT FOR MERGE`).

Independent design ground below — not a re-stamp of Atlas commit prose.

---

## 1. Live board (this tick)

| Surface | Tip / state |
|---------|-------------|
| macOS **#110** | tip **`7f59b35`** · OPEN · MERGEABLE · audit+swarm×4+aggregate **SUCCESS** · **NEW residual** past measured `207a04e` |
| macOS **#130** | tip **`11f4a35`** · OPEN · MERGEABLE · CI SUCCESS · P0a trench gates (instrument; not this unit) |
| macOS **#131** | **MERGED** → develop (was Argos GREEN / design CLEAR) |
| macOS **#132** | **MERGED** → develop (Prometheus R1 CLEAR @ `b73dd7e` banked) |
| macOS **flip** | branch `atlas/subpixel-flip` @ **`7863e81`** · **no PR** · author-marked NOT FOR MERGE |
| macOS master / develop | **`44389f1`** / **`7f59b35`** |
| Win | open **#33 HOLD only** @ `d12321d` · develop `563bc1e` · master `f0c2f5a` |
| Linux **#59** | OPEN @ `7ad1eb0` · CLEAR body banked (no product tip re-open this tick) |
| umbrella **#11** | OPEN · HARD AMEND banked |
| tank | open **zero** |

Preferred land order was #131 then #132; actual merge order was **#132 then #131**. Structurally fine: #132 does not consume `GlyphKey.subpixel_phase`; both freeze production at phase 0 / raster `0.0`.

---

## 2. Unit A — #110 residual @ `7f59b35`

### 2.1 Scope (only)

```
ahead of 207a04e (prior measured tip):
  b73dd7e  feat(text): rasterize glyphs at a subpixel x-offset… (#132 body)
  e240ffa  Merge pull request #132
  7f59b35  feat(renderer): subpixel phase in the glyph key… (#131)

files (207a04e..7f59b35):
  crates/rustkit-renderer/src/glyph.rs  +197/−…
  crates/rustkit-renderer/src/lib.rs    +8
  crates/rustkit-text/src/macos.rs      +167/−…
```

No other product residual on the promote tip past the last banked FontLoader residual (#128+#129 @ `207a04e`).

### 2.2 Independent freeze census (develop tip)

| Site | State @ `7f59b35` |
|------|-------------------|
| Production `GlyphKey` (lib.rs ×2) | **`subpixel_phase: 0`** (explicit freeze) |
| `subpixel_phase_for` production callers | **none** (tests only) |
| macOS `get_or_rasterize` → raster | **`rasterize_char(cp, 0.0)`** — ignores `key.subpixel_phase` |
| Color path | phase irrelevant; still phase-0 bitmaps |
| CG flags (#132) | positioning ON · quantization OFF (load-bearing for 4-phase *capability*) |
| Fraction ownership (#132) | raster consumes offset; `bearing_x` UNSHIFTED |
| Placement | still `cursor_x + entry.offset[0]` (fractional place — correct while phase frozen) |

**Thrash door remains closed:** multi-phase keys are not emitted in production, so the bridge not wiring phase→offset cannot mint 4× identical atlas entries.

### 2.3 Local + CI

| Check | Result |
|-------|--------|
| `cargo test -p rustkit-text --lib` | **68/68** |
| `cargo test -p rustkit-renderer --lib glyphs_at_different_phases…` | **PASS** |
| #110 CI (audit + swarm×4 + aggregate) | **SUCCESS** |
| `git merge-tree --write-tree origin/master origin/develop` | write-tree hash returned · **no conflict** |

### 2.4 Rulings — Unit A

| Item | Ruling |
|------|--------|
| #110 residual @ `7f59b35` | **DESIGN CLEAR / APPROVE** |
| Prior CLEAR @ `a4c0053` / FontLoader body / #131 / #132 | **STAND** for those SHAs |
| Call-site flip on develop | **ABSENT** (correct) |
| 59% / text residual moved by #131+#132 alone | **HARD NO claim** (flip measure says attribution text_metrics byte-identical even with flip) |
| Merge of #110 | **Atlas + Pete** — not Prometheus |

---

## 3. Unit B — flip tip `7863e81` (implementation CLEAR · merge HARD NO)

### 3.1 Scope

```
1 commit ahead of develop 7f59b35:
  7863e81  flip: subpixel phase at the call sites — MEASURED, FALSIFIED, NOT FOR MERGE

files (+27/−12):
  crates/rustkit-renderer/src/glyph.rs   phase → subpixel_x wire
  crates/rustkit-renderer/src/lib.rs     call-site flip + floor place + color phase-0 pin
```

No open PR. Branch is evidence, not a merge candidate.

### 3.2 Implementation ground (CONFIRMED independent)

| Contract from sequencing pin | Tip state |
|------------------------------|-----------|
| Key emits `subpixel_phase_for(cursor_x)` | **YES** — both grayscale production sites |
| Bridge: `subpixel_x = phase as f32 / SUBPIXEL_QUANTIZE` | **YES** — `get_or_rasterize` macOS path |
| Place at `floor(cursor_x) + bearing` (not fractional cursor) | **YES** — three place sites (`floor` + `entry.offset[0]`) |
| Color/emoji pinned phase 0 | **YES** — `GlyphKey { subpixel_phase: 0, ..key }` before color raster |
| Double-application footgun closed | **YES** — fraction owned by raster bitmap only |
| Windows DW subpixel | **out of scope** (unchanged) |

**Implementation: DESIGN CLEAR.** This is the lawful third unit the prior pin described. Atlas built it correctly.

### 3.3 Measurement claim (accepted as ground for merge ruling)

Author-attached receipt (commit body; not re-run this seat — design seat, no parity re-spend):

| Corpus / metric | Before → After | Read |
|-----------------|----------------|------|
| article-typography `diffPercent` | 9.6152% → 9.6319% | flat / noise-worse |
| article-typography `>128` tail | 4.190% → 4.119% | −1.7% relative; mid buckets slightly worse |
| micro | 5.1% → 5.1% | no move |
| websuite | 9.4% → 9.4% | no move |
| `taxonomy.text_metrics` | **94.18515625** byte-identical | attribution class unmoved |
| phase histogram (article-typography) | ~1322 / 1232 / 1198 / 1298 | path live, near-uniform |

**Prediction under test:** horizontal subpixel phase is a *dominant* driver of the text residual / bimodal tail.  
**Result:** path works; effect negligible; cost up to 4× atlas entries per glyph.

### 3.4 Rulings — Unit B

| Item | Ruling |
|------|--------|
| Flip implementation correctness | **CLEAR** |
| Flip as merge to develop/master | **HARD NO** without a new reason (measurement falsified the major-driver claim) |
| Evidence branch `atlas/subpixel-flip` | **KEEP** (do not delete; citation for the falsification) |
| Open a PR to land this tip | **HARD NO** |
| #131 + #132 on develop | **STAND** — key inert at 0; CG quantize fix remains valuable (micro 5.2→5.1 on #132 alone) |
| Horizontal subpixel as major residual driver | **FALSIFIED** (this metric class) |
| Pete wavy-text symptom | **NOT FALSIFIED** — may be vertical / shape / gamma / stem-darkening; different measurement |
| 59% language on any flip PR body | **HARD NO** (number did not move) |

### 3.5 Soft nits (non-blocking; only if tip ever reopens)

1. Production comments on develop still say "FROZEN AT 0 until the rasterizer can draw at a phase" — stale after #132; cosmetic.
2. `GlyphKey` doc still says flip "belongs in the same commit as the rasterizer" — sequencing was split; historical.
3. If a future reason reopens flip (e.g. LCD filtering or a different metric that *does* move), land only with a fresh parity receipt, not this SHA's claim set.

---

## 4. Next strategic slice (text residual after horizontal falsification)

Horizontal grayscale phase is **closed as a campaign**. Do **not** thrash more phase buckets.

| Candidate | Why now | Owner sketch |
|-----------|---------|--------------|
| **Glyph shape / CT vs Skia (Chrome)** | Same ink silhouette may differ; residual often lives in stem width / contrast | Atlas measure: side-by-side single-glyph PNG at identical size/weight; diff mask |
| **Gamma / coverage → color** | Text metrics attribution high while phase flat → blending curve mismatch | Athena/Atlas: document current coverage pipeline; one controlled gamma experiment |
| **Stem darkening / font smoothing** | Platform defaults differ; can look "wavy" or heavy without phase error | Research pin first; no code until measured |
| **Vertical phase / baseline snap** | Constant within a run, varies between lines; untested; closer to "wavy" report | Design pin before code; one-line vertical offset harness |
| **#130 P0a gates** | Instrument only; unblocks forensic attribution of *next* residual | Atlas land when CI green; not a product pixel claim |
| **#110 promote land** | Residual CLEAR; content-input campaign complete enough for Pete go | Atlas + Pete |

**Prometheus will not** open a "subpixel phase 2" unit without a new measurement that re-opens the claim.

---

## 5. What this seat will not do

No merge to master/develop, no force-push, no branch delete, no spend, no history rewrite, no null attend.  
Atlas owns merge decisions; Athena owns portable follow-ons; Argos may re-GREEN tips independently.

---

## 6. Artifacts

- This file (uncommitted macos docs lane — Atlas PR lane if desired)
- Exchange doorbell-note this tick
- WORK_QUEUE.md updated

— prometheus (Grok / design seat, scheduled grind)
