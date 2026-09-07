# n45 — what each open PR is worth in the campaign's own metric

**NOT A RECEIPT.** Every number here is Linux/SwiftShader with fontconfig
substitution, not macOS/CoreText/Metal. The metric is defined against the pinned
`baselines/chrome-148` set on macOS alone (`trench/BASELINE-parity-finish-line.md`).
"Geometry-green" below is **finish-line condition 1 of 4**, never an `N/26`.

**Date:** 2026-09-07 · **Basis:** `develop 5b89ed8` · **Gate:** one pinned
instrument (develop's `scripts/layout_oracle_gate.py`) over every row, so only
the engine differs between columns.

---

## Why this board exists

The night order sent this seat at `combinators` — night 44's hand-off, "the
cleanest small root on the board, fully diagnosable here, and no open branch
touches it." The second half is false, and finding that out took ten minutes:

| combinators, `develop 5b89ed8` → `#185` | develop | #185 |
|---|---:|---:|
| Gate A geometry failures | 25 | **0** |
| boxes compared | 41/41 | 41/41 |
| geometry-green (condition 1) | no | **yes**, stable ×3 |
| Gate B paint within ±5 | 92.7059% | 93.2236% |
| Gate B elements examined / withheld | 20 / 21 | **41 / 0** |
| discrete structural auto-fails | 0 | 0 |

`#185`'s own PR body reports this case as **`combinators −0.53`** on the
mean-pixel campaign board — a figure that reads like rounding noise. What it
actually did was take a case from 25 geometry failures to zero and hand Gate B
its entire jurisdiction on that case (20 admissible elements → 41). Night 44
also measured `combinators` at **zero seat confound**, so this one is not a
Linux artefact: it holds on macOS.

That is §1 of the plan — the scoreboard lying — reappearing one level up. The
gates are honest now; **the PR prose that summarises them is not**, because it
still quotes the mean-pixel board the gates were built to replace.

So the night's unit became: measure every open engine branch, alone, in the
metric's own currency.

## Method

Each branch checked out detached, `cargo build --release -p parity-capture`, all
26 gating cases captured (9.5s a set) with
`VK_ICD_FILENAMES=/opt/pw-browsers/chromium-1194/chrome-linux/vk_swiftshader_icd.json`.
Gate A then run once per capture set **from a single pinned develop checkout**,
so the instrument is constant and only the engine varies. Counts and magnitude
are reported as a pair throughout; neither alone is admissible.

## The board — Gate A, each branch measured ALONE against develop

`geom+join` failures per case. `GREEN` = condition 1 met.

| case | develop | 174 | 175 | 176 | 178 | 179 | 180 | 181 | 182 | 183 | 184 | 185 |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| about | 405 | 405 | 405 | 404 | 405 | 405 | 405 | 405 | **383** | 405 | 405 | 405 |
| article-typography | 97 | 97 | 97 | 97 | 97 | 97 | 97 | 97 | 97 | 97 | 97 | 97 |
| backgrounds | 53 | 53 | 53 | 53 | 53 | 53 | 53 | 53 | 53 | 53 | 53 | 53 |
| bg-pure | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN |
| bg-solid | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 12 |
| card-grid | 150 | 150 | 150 | 150 | 150 | 150 | 150 | 150 | 150 | 150 | 150 | 150 |
| chrome_rustkit | 49 | 49 | 49 | 49 | 49 | 49 | 49 | 49 | 49 | 49 | **45** | 49 |
| **combinators** | 25 | 25 | 25 | 25 | 25 | 25 | 25 | 25 | 25 | 25 | 25 | **GREEN** |
| css-selectors | 108+5 | 108+5 | 123+0 | 108+5 | 108+5 | 108+5 | 108+5 | 108+5 | 108+5 | 108+5 | 108+5 | 108+5 |
| flex-positioning | 156+7 | 156+7 | 176+0 | 156+7 | 156+7 | 156+7 | 156+7 | 156+7 | 156+7 | 156+7 | 154+7 | 156+7 |
| form-controls | 54+30 | 54+30 | 110+4 | 54+30 | 54+30 | 54+30 | 54+30 | 54+30 | 54+30 | 54+30 | 54+30 | 54+30 |
| form-elements | 87+17 | 87+17 | 124+1 | 87+17 | 87+17 | 87+17 | 88+17 | 87+17 | 87+17 | 87+17 | 87+17 | 88+17 |
| gpu-gradient-regression | 132 | 132 | 132 | 132 | 132 | 132 | 132 | 132 | 132 | 132 | 132 | 132 |
| gradient-backgrounds | 82 | 82 | 82 | 82 | 82 | 82 | 82 | 82 | 82 | 82 | 82 | 82 |
| gradient-no-radius | 14 | 14 | 14 | 14 | 14 | 14 | 14 | 14 | 14 | 14 | 14 | 14 |
| gradient-radius-only | 14 | 14 | 14 | 14 | 14 | 14 | 14 | 14 | 14 | 14 | **17** | 14 |
| gradients | 56 | 56 | 56 | 56 | 56 | 56 | 56 | 56 | 56 | 56 | 56 | 56 |
| image-gallery | 155 | 155 | 155 | 155 | 155 | 155 | 155 | 155 | 155 | 154 | 155 | 155 |
| images-intrinsic | 37+14 | 33+14 | 50+0 | 37+14 | 37+14 | 37+14 | 37+14 | 37+14 | 37+14 | 37+14 | 37+14 | 37+14 |
| new_tab | 243+1 | 243+1 | 245+0 | 243+1 | 243+1 | 243+1 | 243+1 | 243+1 | **220+1** | 243+1 | 242+1 | 243+1 |
| pseudo-classes | 32 | 32 | 32 | 32 | 32 | 32 | 32 | 32 | 32 | 32 | 32 | 32 |
| rounded-corners | 67 | 67 | 67 | 67 | 67 | 67 | 67 | 67 | 67 | 67 | 67 | 67 |
| settings | 344+31 | 344+31 | 434+8 | 344+31 | 344+31 | 344+31 | 344+31 | 344+31 | 344+31 | 344+31 | 344+31 | 344+31 |
| shelf | 14+4 | 11+5 | 18+2 | 14+4 | 14+4 | 14+4 | 14+4 | 14+4 | 14+4 | 14+4 | 12+4 | 14+4 |
| specificity | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN | GREEN |
| sticky-scroll | 114+1 | 114+1 | 116+0 | 114+1 | 114+1 | 114+1 | **149+1** | 113+1 | 114+1 | 114+1 | 114+1 | 114+1 |
| **geometry-green /26** | **2** | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | **3** |
| corpus geometry failures | 2500 | 2493 | 2739 | 2499 | 2500 | 2500 | 2536 | 2499 | 2455 | 2499 | 2494 | 2476 |
| corpus join failures | 110 | 111 | **15** | 110 | 110 | 110 | 110 | 110 | 110 | 110 | 110 | 110 |
| corpus `sum\|Δ\|` | 239566 | 239568 | 249918 | **106594** | 239566 | 239566 | 265954 | 236772 | 247016 | 235160 | 226802 | 240156 |

## What the board says

**1. `#185` is the only open branch that turns any case geometry-green.** Ten
other engine branches move failure counts by single digits and none crosses a
case's threshold. In the campaign's metric the pile is worth exactly one
condition on one case — and that one is real (zero confound) and stable ×3.

**2. `#176` is the largest magnitude item in the pile by an order of magnitude,
and the count metric cannot see it.** Corpus `sum|Δ|` 239566 → **106594, −55%**,
while its failure count moves by one (about 405 → 404). A single atomic-inline
`width:auto` shrink-to-fit fix is over half the corpus's geometry error. Nothing
in its PR title suggests that, and no per-case count would ever surface it.

**3. `#175` is the jurisdiction unlock, and its "+239 failures" is not a
regression.** Join failures **110 → 15** corpus-wide: ~95 elements that had no
box at all now join and are compared for the first time, then fail. This is
exactly the case `#172`'s ratchet language exists for — *a widened jurisdiction
is not a regression*. Read without it, `#175` is the worst branch on the board;
read with it, it is the one that makes the form cases measurable at all.

**4. `#180`, measured alone, regresses `sticky-scroll` hard.** `sum|Δ|` 4518 →
**32016** (7×), failures 114 → 149, and `form-elements` 87 → 88. Under the stop
rule as written, per PR, `#180` is auto-revertable. It is also the branch
carrying the one real semantic conflict in the pile (against `#176`, both
rewriting the `Length::Auto` arm of `calculate_block_width`). This is the
`#182`/`new_tab` shape from 09-05 again: correct-looking change, regression when
measured alone, unknown when measured stacked.

**5. `#181` is right about its own number and it buys nothing on the metric.**
`sticky-scroll` `sum|Δ|` 4518 → 1724 is the **−62%** its PR title claims,
exactly. Failure count 114 → 113. Boxes moved much closer to Chrome without
crossing 0.5px. Magnitude and count are different questions and the finish line
asks the second one.

**6. `#178` and `#179` move Gate A by exactly zero here, and that is not
evidence about them.** `#178` is the engine half of a pair whose gate half
(`#177`) changes what Gate A joins on; measured with develop's gate it *cannot*
show its effect. `#179` is a fallback-face/line-height fix and this seat has no
CoreText and no Georgia. Both rows are **NOT MEASURED HERE**, not "no value".

## The one uncovered case, and why it is not workable here

`card-grid` — 150 failures, `sum|Δ|` 1504.21, **byte-identical across all eleven
branches** (only `#184` moves it, 1504.21 → 1406.18, count unchanged). The only
sizeable case in the corpus no open branch touches.

Night 44's seat control says why it is untouched rather than merely unlucky:

```
card-grid   reported 1504.21   real 1700.41   confound 706.48   47.0% is the seat
            buckets: real=43  mixed=103  confound=4  masked=0
```

47% of what Gate A reports on this case here is Linux, and the real residual is
*larger* than the reported one — the confound partially masks the defect. The
worst surviving real deltas are all `.stats > span` widths (46.7, −38.8, −34.4,
−33.8) — **text advance widths**, which is P4, the item defined by needing
CoreText on both sides. Same wall night 44 hit on `article-typography`.

Reproduced with:
```
node tools/parity_oracle/capture_seat_control.mjs --case card-grid   # PARITY_CHROME_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome
python3 scripts/seat_control_report.py --layout-root <captures> --case card-grid --verbose
```
(Both from `#186`. The seat's Playwright wants `chromium_headless_shell-1200`
and only `-1194` is installed; the existing `PARITY_CHROME_PATH` env var covers
it, no patch needed.)

## Reproduction

`trench/tools/n45_capture_all.py` and `trench/tools/n45_condition_board.py`.
They are **diagnostics with no mutation-checked guards** and must not be wired
into CI; they only aggregate Gate A's own JSON and cannot invent a number, but
their `geometry-green /26` line is condition 1 and would be misread as `N/26` by
anyone who did not read this header.
