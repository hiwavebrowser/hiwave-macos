# n44 — how much of the trench seat's Gate A error is the trench seat

**Date:** 2026-09-06 · **Seat:** Linux x86-64, SwiftShader, fontconfig
**Tree:** `develop 5b89ed8`, unmodified · **Tool:** `scripts/seat_control_report.py`

> **NOT A RECEIPT.** Every number here is Linux. The campaign metric is defined
> against `baselines/chrome-148/` with RustKit on macOS, and nothing below is an
> `N/26`.

## Why this exists

Night 4 (2026-08-04) recorded that the trench seat could not separate real
RustKit defects from platform noise, and that "the split needs a macOS run to
make, not a cleverer analysis of this one". Nights since have carried that as a
standing caveat on every Gate A board from this seat.

It needs neither a macOS run nor a cleverer analysis. It needs a **control**:
Chrome captured *on this seat*, through the same `captureBaseline` code that
produced the pinned set, so the fonts and the browser are identical on both
sides and the only thing left is box math.

```
Δ_confound = Chrome_seat  − Chrome_pinned    the seat
Δ_real     = RustKit_seat − Chrome_seat      the defect
Δ_reported = RustKit_seat − Chrome_pinned    what Gate A prints here
```

Seat control: Chromium 141.0.7390.37 (Playwright 1.57.0) against the pinned set's
Chrome 148 on macOS. `Georgia -> DejaVuSerif.ttf`, `-apple-system -> DejaVuSans.ttf`,
`Helvetica`/`Arial` -> `LiberationSans-Regular.ttf`.

**The control folds font substitution and the 141-vs-148 build difference into a
single term.** Nothing here separates those two, and nothing needs to: both are
the seat, and neither is RustKit.

## The board — `develop 5b89ed8`, all 26 gating cases

`reported` and `real` are `sum|Δ|` over axes past the gate's 0.5px tolerance.
`share` is `confound / reported`.

| case | reported | real | confound | share | real | mixed | confound | masked |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| about | 147636.02 | 152508.59 | 8368.86 | 5.7% | 98 | 306 | 1 | 6 |
| new_tab | 26434.06 | 26400.47 | 329.75 | 1.2% | 130 | 113 | 0 | 0 |
| settings | 26046.62 | 24219.65 | 3245.00 | 12.5% | 70 | 274 | 0 | 0 |
| image-gallery | 12545.39 | 12604.66 | 152.05 | 1.2% | 48 | 107 | 0 | 0 |
| article-typography | 6056.65 | 6117.61 | 3142.70 | **51.9%** | 36 | 61 | 0 | 13 |
| sticky-scroll | 4518.28 | 4950.94 | 537.75 | 11.9% | 18 | 95 | 1 | 10 |
| flex-positioning | 3821.26 | 3406.93 | 551.22 | 14.4% | 22 | 134 | 0 | 0 |
| css-selectors | 2926.54 | 1956.83 | 1118.89 | 38.2% | 5 | 103 | 0 | 1 |
| card-grid | 1504.21 | 1700.41 | 706.48 | 47.0% | 43 | 103 | 4 | 0 |
| form-elements | 1504.02 | 1269.23 | 365.48 | 24.3% | 17 | 69 | 1 | 1 |
| rounded-corners | 1163.73 | 1063.51 | 205.91 | 17.7% | 1 | 34 | **32** | 0 |
| images-intrinsic | 1046.24 | 893.16 | 242.00 | 23.1% | 3 | 25 | 9 | 0 |
| gradient-backgrounds | 948.59 | 721.22 | 227.38 | 24.0% | 20 | 62 | 0 | 0 |
| gpu-gradient-regression | 542.48 | **45.40** | 480.00 | **88.5%** | 0 | 50 | **82** | 0 |
| form-controls | 458.54 | 480.31 | 395.53 | **86.3%** | 17 | 37 | 0 | 0 |
| chrome_rustkit | 445.94 | 407.16 | 46.91 | 10.5% | 8 | 34 | 7 | 1 |
| gradient-radius-only | 382.09 | 290.27 | 91.83 | 24.0% | 0 | 14 | 0 | 0 |
| gradients | 341.02 | **83.59** | 252.03 | **73.9%** | 0 | 37 | 19 | 0 |
| gradient-no-radius | 320.40 | 228.10 | 92.30 | 28.8% | 0 | 14 | 0 | 0 |
| backgrounds | 315.02 | **73.19** | 236.31 | **75.0%** | 0 | 34 | 19 | 0 |
| pseudo-classes | 274.50 | 209.25 | 65.25 | 23.8% | 0 | 32 | 0 | 0 |
| combinators | 160.00 | 160.00 | **0.00** | **0.0%** | 25 | 0 | 0 | 0 |
| shelf | 139.78 | 130.86 | 12.42 | 8.9% | 5 | 8 | 1 | 0 |
| bg-solid | 34.96 | **8.72** | 23.72 | 67.8% | 0 | 3 | 9 | 0 |
| bg-pure | 0.00 | 0.00 | 0.00 | 0.0% | 0 | 0 | 0 | 0 |
| specificity | 0.00 | 0.00 | 0.00 | 0.0% | 0 | 0 | 0 | 0 |
| **TOTAL** | **239566.34** | **239930.04** | **20889.77** | **8.7%** | 566 | 1583 | 185 | 32 |

Bucket meanings, per failing axis: **real** — fails both comparisons at the same
magnitude; **mixed** — fails both, but the reported magnitude is not the
defect's; **confound** — fails against the pinned set only, i.e. the seat;
**masked** — passes against the pinned set and fails against the control.

## What it says

**1. The confound is 8.7% of the corpus, not the majority.** Night 4's reading
of the axis histogram — "the vertical lean is consistent with the Linux font
stack" — is measured and does not hold at corpus scale. 2149 of 2334 failing
axes survive the control. The trench seat's Gate A boards on the large roots are
substantially trustworthy, and have been all along.

**2. It is concentrated, and exactly inverted from where the effort has gone.**
The four largest cases — `about`, `new_tab`, `settings`, `image-gallery`, which
between them are 88% of the corpus error — carry 1.2%–12.5% confound. The
*small* cases are where the seat dominates: `gpu-gradient-regression` 88.5%,
`form-controls` 86.3%, `backgrounds` 75.0%, `gradients` 73.9%, `bg-solid` 67.8%.
`rounded-corners` keeps most of its magnitude but 32 of its 67 failing axes are
pure seat.

Concretely: **geometry work on `gpu-gradient-regression`, `gradients`,
`backgrounds` and `bg-solid` should not be driven from this seat.** Their real
residual error is 45.40, 83.59, 73.19 and 8.72 — small enough that chasing the reported
number would mostly be chasing Chromium 141.

**3. `article-typography` is 51.9% seat, which is the expected result and the
first time it has been a number.** P4 (text advance widths) is the one queue item
that genuinely cannot be driven from here: half its error is the substitution,
and the surviving half is measured against DejaVu advances, not CoreText.

**4. `combinators` is 0.0% confound: 25 failing axes, every delta exactly
−10.00px, nothing else in the case.** A fully seat-diagnosable defect with a
constant offset — the cleanest small root on the board, and no open branch
touches it.

**5. `masked` is a real category and it is not empty — 32 axes.** These pass
against the pinned macOS baseline and fail against the control: RustKit's error
and the platform's error cancel. `article-typography` 13, `sticky-scroll` 10,
`about` 6. On a macOS seat those become visible failures. **They are latent
regressions that this seat's Gate A currently scores green**, which is the
opposite of the concern the seat has been carrying.

**6. `sticky-scroll`'s two sticky sidebars are 1406.56 and 1395.27 real with
0.00 confound** — both `aside`s take the grid row's full height instead of
`height: fit-content`. That is PR #181's subject, and the control confirms it is
box math rather than a font artefact.

## Validation

- The confound term was computed independently, directly from the two
  `layout-rects.json` sets, before the script existed: 20889.77 over 2101 axes.
  The script reproduces it to the last decimal.
- The reported term reproduces `layout_oracle_gate.py`'s own board on the same
  captures: 239566.34, and `about` 147636.02 — matching 09-04's and 09-05's
  develop figures on this seat.
- **Zero selector mismatches across all 26 cases** between Chrome 148/macOS and
  Chromium 141/Linux. The P0a-0 join key is stable across platform *and* seven
  Chromium milestones — a stronger result than
  `verify_selector_key.mjs` alone establishes.

## Deliberately not wired into CI

The gating lanes run on `macos-14`, where the control would be measuring Chromium
141 against Chrome 148 with no font difference — a smaller, differently-shaped
number that answers no question the receipt lane has. The tool is for non-macOS
seats. Adding it to `parity.yml` would spend capture minutes to produce a figure
nobody should quote.
