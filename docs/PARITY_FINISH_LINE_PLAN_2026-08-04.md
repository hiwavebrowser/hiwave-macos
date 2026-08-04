# macOS Chrome-parity finish line — campaign plan

> **Status:** DRAFT — awaiting Pete's approval to hand to the trench.
> **Authors:** Atlas (assessment + plan) · Prometheus (independent attack + amendments, exchange reply 2026-08-04). Disagreements are recorded, not smoothed over.
> **Scope:** macOS only. This finish line is **engine correctness**, not product launch. Windows/Linux port their own flavors of the defect classes; they do not join this re-instrument.
> **Exists in service of:** HiWave.

## 0. Where we are (measured, master `962efc1`, all 26 cases)

mean 6.64% · median 5.35% · worst 14.44% · 3 cases < 2% · bg-pure 0.00%.
Four weeks ago the worst case was 50.4%. Full board in §8.

Honest translation, three layers (Prometheus's split, adopted):

| Layer | Estimate | Evidence |
|---|---|---|
| Pure paint | ~100% | bg-pure 0.00, bg-solid 1.61, gradients 1.06 |
| 26-case corpus | ~93% raw pixel agreement; ~55–65% of cases structurally honest if re-scored on geometry | board is bimodal: cluster 0–5%, tail 10–14% of layout/clip roots |
| Real-page product feel | **~70–85%** (Prometheus) vs ~93–97% (Atlas) — unresolved until a live holdout board exists | corpus is a microscope, not a telescope |

**The disagreement is recorded on purpose.** Atlas extrapolated from the built-in UI pages (new_tab 2.8%, settings 4.05%); Prometheus refuses to sign any product number without a 10-site live holdout. He is right that the corpus cannot answer the product question; the holdout board (P5) settles it.

## 1. The core finding: the scoreboard is lying (Goodhart)

Master's shelf — the whole UI collapsed into a 141px column — scored **3.71% (pass)**. The geometrically correct tree scored **33.87% (fail)**, because correct widths exposed a paint bug across more pixels. The metric preferred a broken layout. Three instrument failures in one week (this; #84 decorative attribution; stability flag never enforced at PR level) make **re-instrumentation P0 — a program risk, not chore debt.**

Literal bit-parity with Chrome is the wrong north star: Chrome is not bit-stable against itself (text AA, gradient dither, resample kernels). But **geometry can be bit-exact** — box math, not rasterization — and Chrome's layout-rects are already committed in `baselines/chrome-148/`.

## 2. The oracle (dual + forensic, Prometheus-amended)

| Gate | What | Bar | Role |
|---|---|---|---|
| **A Geometry** | RustKit `layout.json` vs `chrome-148 layout-rects.json` | ≤ 0.5px per box, per-box attribution on fail | Primary grind driver |
| **B Paint** | per-channel tolerance (pin ONE constant in VISUAL_DIFF_POLICY + parity_gate — no floating duplicates) | ≥ 99% within tolerance; **discrete structural failures (paint-outside-box, missing clip, wrong solid color) auto-FAIL regardless of %** | Keeps real paint bugs loud; stops AA noise counting |
| **C Forensic** | full raw pixel heatmap + worst-N | **non-gating**, published on every PR | Catches what A+B can miss: stacking/z-order, shadows/outlines/selection not in rects, resample kernels |
| Stability | 3 iterations, enforced at pr_merge **and** nightly | closes the `stable:false`-never-gates hole in parity_gate.py | |

Ground rules: no engine behavior change in the re-instrument PRs (metric change must be attributable). Mean-diff-only "wins" banned from PR descriptions — report geometry+paint pairs.

## 3. Finish line (ratified definition)

Before any corpus expansion, all 26 cases must hold **simultaneously**:
1. Geometry-exact (≤ 0.5px every box).
2. Paint ≥ 99% within the pinned tolerance.
3. Stable across 3 iterations.
4. Zero discrete structural failures, even where % would pass.
5. Forensic board published (non-gating).
6. Holdout suite: canary-only until 1–4 are green, then promote the 2 worst holdouts into the gate set.

**Rejected as ship bar:** mean pixel-diff ≤ N% alone; "93% so basically done"; corpus expansion that dilutes the mean; the 100%-raw-pixel goal in `100pct-pixel-parity-plan.md` (acceptance criteria change; the ruthlessness stays).

## 4. Campaign order (trench queue)

**P0a — Re-instrument** (split oracle A/B/C + stability at pr_merge). No engine changes riding along.
**P0b — Dual-oracle baseline receipt on master** — one commit stating geometry-fail count and paint-fail count. New ground truth Pete can trust.
**P1 — Gradient/clip family** (gradient-backgrounds 14.44, -no-radius 13.96, -radius-only 10.77, gpu-gradient-regression 5.24). First root already landed as #86 (scaled gradient painted unclipped — PushClip fix, mutation-checked). Remaining: rounded clip for scaled gradients (corner notches), the -no-radius/radius-only residuals, interpolation color match.
**P2 — Grid/sticky family** (sticky-scroll 11.71, card-grid 7.25). The `1fr` min-content floor diagnosis from 07-08 gets *finished*, not re-theorized.
**P3 — Flex residual post-#85** (flex-positioning 10.80). Likely sibling class: alignment/baseline/absolute-in-flex. Flex is not "done".
**P4 — Text advance widths** (article-typography 9.62; css-selectors 12.09 is cascade+text mixed). CoreText on both sides ⇒ metric-exact advances are achievable; AA stays under gate B tolerance.
**P5 — Images family** (images-intrinsic 9.35, image-gallery 6.89) + **10-site live holdout board** (non-gating) — settles the product-feel number.
**P6 — Forms/UA + about's cyan selection artifact** (form-controls 6.42, form-elements 5.13, about 13.14). Selection artifact is likely **paint-order/stacking**, a family Atlas's first draft missed entirely — it rides gate C until it earns a case.

Not on the critical path (banned from interleaving): dead_code cleanups, MCP work, cross-platform ports.

## 5. Trench configuration (on approval)

- **Metric:** cases passing the FULL finish-line conjunction (geometry ∧ paint ∧ stable ∧ no-discrete), i.e. `N/26 finish-line-green` — not mean diff.
- **One repo:** hiwave-macos. **One queue:** §4 order. Baton: "land the next P-item," never "lower the mean."
- **Noon digest:** N/26, the P-item in flight, any oracle disagreement (a case geometry-green but paint-red is signal, not noise).
- **Stop rule:** any change that improves the metric while any oracle regresses on any case → auto-revert, log as mistake.

## 6. Fleet: hallmark defect classes for OS ports

Each class shipped on macOS with a mutation-checked T-RED. Port seats verify their flavor — the *check*, not the assumption:

| Class | macOS receipt | Port note |
|---|---|---|
| Explicit cross size floored by intrinsic (flex) | #85 | Prometheus: Windows lacks this exact path — verify, don't assume; Linux unmeasured |
| Column-flex cross-axis measured on wrong axis | #82 (pair) | pairs with transparent-bg paint fix |
| Form controls paint white on `background: transparent` | #83 | UA-default-in-engine pattern, painter can't distinguish unset from transparent |
| Scaled/offset/tiled gradient paints outside its box | #86 | check every `render_background_layer` flavor; "viewport will clip it" is the tell |
| Stretch targets containing block instead of container inner width | dba3d53 (in #82) | #81 inverted |
| Instrument classes: metric prefers broken layout · stale attribution (#84) · unenforced stability | this doc §1 | every seat's parity harness should be audited for all three |

## 7. Approval asks (Pete)

1. Ratify the finish line (§3) — this retires "100% raw pixel parity" as the acceptance bar.
2. Approve the trench on §5 config with the §4 queue.
3. The product-feel number stays unsigned until P5's holdout board exists — accept ~70–85% as the honest working estimate until then.

## 8. Appendix — full board (master 962efc1)

gradient-backgrounds 14.44 · gradient-no-radius 13.96 · about 13.14 · css-selectors 12.09 · sticky-scroll 11.71 · flex-positioning 10.80 · gradient-radius-only 10.77 · article-typography 9.62 · images-intrinsic 9.35 · card-grid 7.25 · image-gallery 6.89 · form-controls 6.42 · specificity 5.45 · gpu-gradient-regression 5.24 · form-elements 5.13 · combinators 4.67 · settings 4.05 · shelf 3.62 · pseudo-classes 3.51 · backgrounds 3.41 · rounded-corners 3.33 · new_tab 2.80 · chrome_rustkit 2.37 · bg-solid 1.61 · gradients 1.06 · bg-pure 0.00
