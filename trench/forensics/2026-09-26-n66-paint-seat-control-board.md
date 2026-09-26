# Paint seat control — the confound Gate B never had a name for

**Date:** 2026-09-26 (night 66) · **Seat:** Linux x64, SwiftShader, no system text backend
**Tree:** `develop a0176dd` · **Instrument:** `scripts/seat_control_paint_report.py`
**Captures:** 26/26 gating cases, `parity-capture` release, one iteration
**Control:** `baselines/seat-control`, captured 2026-09-26T05:14:14Z, playwright 1.57.0,
chromium_headless_shell-1194, `Georgia -> DejaVuSerif.ttf`

> **NOT A RECEIPT.** Nothing in this file is an `N/26`. Every figure is
> Linux/SwiftShader. The metric is defined against the pinned macOS set alone.

---

## Why this exists

Night 44 built a seat control for **geometry** and it has run ever since:
`Δ_reported = Δ_real + Δ_confound`, with the confound measured by capturing
Chrome on the seat itself. Gate B never got one. Its percentages were taken on
this seat for twenty-two nights and treated as roughly indicative, with no
number attached to "roughly".

The control already had the pixels. `capture_seat_control.mjs` reuses
`captureBaseline`, which writes a `baseline.png` per case. Nothing read them.

## The board

```
case                         reported   confound       real     kept    masked     floor
about                        20.5375%   15.1073%   23.5683%   84.89%  14.6067%  14.6067%
article-typography           16.9044%    7.9168%   16.6627%   92.08%  10.7149%   7.9168%
backgrounds                  11.5798%   11.5649%    6.6809%   88.44%   2.8318%   2.8318%
bg-pure                       0.0000%    0.0000%    0.0000%  100.00%   0.0000%   0.0000%
bg-solid                      3.9198%    2.6350%    3.4165%   97.36%   1.7261%   1.7261%
card-grid                    31.2893%    7.1566%   30.7472%   92.84%  28.0313%   7.1566%
chrome_rustkit                4.3469%    1.9891%    4.2375%   98.01%   2.6448%   1.9891%
combinators                   6.7861%    4.3067%    7.0337%   95.69%   3.3460%   3.3460%
css-selectors                48.0285%   32.3751%   44.7683%   67.62%  34.1136%  32.3751%
flex-positioning             39.4329%   18.5116%   35.8259%   81.49%  31.1877%  18.5116%
form-controls                15.1898%    5.7942%   16.2676%   94.21%  11.7920%   5.7942%
form-elements                11.8219%    4.8258%   11.6873%   95.17%   9.0974%   4.8258%
gpu-gradient-regression       8.5629%    6.9380%    4.3868%   93.06%   2.2667%   2.2667%
gradient-backgrounds         15.6127%    4.1515%   11.5554%   95.85%  12.8406%   4.1515%
gradient-no-radius            5.6400%    3.4281%    6.0804%   96.57%   3.0862%   3.0862%
gradient-radius-only          5.3360%    2.9044%    5.6419%   97.10%   3.1711%   2.9044%
gradients                    13.7077%    6.8900%    7.7533%   93.11%   7.9351%   6.8900%
image-gallery                21.6964%    3.5881%   22.6238%   96.41%  19.9652%   3.5881%
images-intrinsic              9.0071%    7.6620%    4.0424%   92.34%   1.8851%   1.8851%
new_tab                      14.6519%    5.5771%   13.7158%   94.42%  11.0128%   5.5771%
pseudo-classes                7.8053%    4.2548%    7.5773%   95.75%   4.3695%   4.2548%
rounded-corners               7.8011%    4.5119%    6.2208%   95.49%   3.9928%   3.9928%
settings                     17.2009%    8.1233%   15.3831%   91.88%  12.3506%   8.1233%
shelf                         7.2487%    2.1882%    6.4186%   97.81%   5.2856%   2.1882%
specificity                   6.6867%    3.7687%    6.6392%   96.23%   3.4011%   3.4011%
sticky-scroll                 5.3886%    2.9926%    5.4374%   97.01%   2.9515%   2.9515%
```

`reported` is what Gate B prints here. `confound` is Chrome-on-this-seat against
the pinned macOS Chrome — two Chromes, same page, no RustKit anywhere near it.
`kept` is the mask: pixels where the two Chromes agree within the pinned ±5.
`masked` is RustKit's disagreement *inside* that mask. `floor` is the smaller of
`confound` and `masked`.

## What it says about the question night 09-25 recorded

09-25 closed by recording the next unit: **are the four sub-0.7pp paint gaps
defects or rasterizer difference?** Answer: **not from this seat, and now that
is measured rather than assumed.**

| case | macOS gap | seat floor | floor ÷ gap |
|---|---:|---:|---:|
| images-intrinsic | 1.064% | 1.885% | **1.77×** |
| gpu-gradient-regression | 1.284% | 2.267% | **1.77×** |
| gradient-no-radius | 1.464% | 3.086% | **2.11×** |
| gradient-backgrounds | 1.652% | 4.152% | **2.51×** |
| pseudo-classes | 1.699% | 4.255% | 2.50× |
| specificity | 1.926% | 3.401% | 1.77× |
| combinators | 2.016% | 3.346% | 1.66× |
| backgrounds | 2.129% | 2.832% | 1.33× |
| rounded-corners | 2.386% | 3.993% | 1.67× |
| flex-positioning | 4.404% | 18.512% | 4.20× |
| card-grid | 17.522% | 7.157% | 0.41× |

macOS gaps are `1 − paint` from run 36100178666 (`macos-14`, 2026-09-25).

**Ten of the eleven paint-blocked cases are below this seat's floor. The one
that is not is `card-grid`, which is 17.5 points from the bar** — the furthest
from green of the eleven. The seat can only work the case nobody would pick.

Before the mask the picture was worse and more misleading: `images-intrinsic`
reads 9.01% here against 1.06% on macOS, a factor of 8.5. The mask takes it to
1.89%, a factor of 1.77. The mask is worth having and it is not enough.

## Three things the board says that were not expected

**1. The three counts are not additive, and `real` exceeds `reported` on six
cases.** `combinators` 7.03 vs 6.79, `form-controls` 16.27 vs 15.19,
`gradient-no-radius` 6.08 vs 5.64, `gradient-radius-only` 5.64 vs 5.34,
`image-gallery` 22.62 vs 21.70, `sticky-scroll` 5.44 vs 5.39. RustKit lands on
the *pinned* value at pixels where it misses the seat's own Chrome, so
`Δ_reported − Δ_confound` is negative there. Any report that subtracted one
count from another would print a negative confound and call it a measurement.
The script never subtracts; two mutation probes exist for exactly this and both
needed a purpose-built fixture before they went red (see below).

**2. `backgrounds` is almost entirely confound: 11.5798% reported against
11.5649% Chrome-vs-Chrome.** 99.87% of what Gate B blames on RustKit for that
case on this seat is two Chromes disagreeing with each other. Its masked
residual is 2.83%, against a macOS gap of 2.13% — the tightest agreement on the
board. That case was never RustKit's here.

**3. `bg-pure` is 0.0000% on all three comparisons.** One case in the corpus is
bit-identical between macOS Chrome, Linux Chrome and RustKit-on-SwiftShader. It
is also the one case that is already finish-line-green. It is the control that
says the pipeline is sound and the confound is font and AA, not a colour-space
or gamma difference running through everything.

## What the mask does not remove

The mask removes the **Chrome** half of the confound. RustKit's own seat
dependence — SwiftShader instead of Metal, and on this seat no system text
backend at all — is still inside `masked`, which is why `masked` is published as
a floor and never as an estimate of what macOS would say. `gradient-backgrounds`
is the clearest case: `confound` 4.15% but `masked` 12.84%, i.e. masking the
Chrome side *raised* the number, because most of that case's disagreement is
RustKit's, and its macOS gap is 1.65%. The seat's RustKit is not macOS's
RustKit either, and nothing here separates those two.

Closing that would need a RustKit-side control — the same engine on both sides
of a comparison, which is what a macOS CI run already is. That is the honest
end of the line for this seat.

## Mutation-check

18 probes, 18 RED, no survivors; control green before and after; bytecode cache
cleared between probes (09-25's banked sweep-validity rule) and every probe run
under `python3 -B`. Full list in the digest.

Two probes survived their first sweep and both were the same fault: the rule was
guarded at the layer that counts pixels and not at the layer that publishes
percentages, and the fixture that caught the first (`reported` derived as
`confound + real`) made the other two derivations land on the right answer by
accident. The fixture is now built so no two of the three counts produce the
third by adding, subtracting or absolute difference: 4 / 5 / 3 on eight pixels.
