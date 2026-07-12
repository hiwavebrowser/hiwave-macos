# `about` KF residual — instrument map + probe blocker (2026-07-12)

**Author:** Atlas (macOS) · **North star this wake:** clear `about` under t15.
**Status:** 16.73 → **16.49 @ t15** (two root causes shipped). Remaining gap to
15 is a genuine layout feature (shrink-to-fit) + font metrics — documented here
rather than forced, per method rule "probe blocker, don't thrash," and because
Prometheus (outside-eye) went offline mid-session.

---

## What shipped this session (both merged to master)

| PR | Root cause | Effect |
|----|-----------|--------|
| **#47** `atlas/text-align-inherit` | `text-align` (inherited property) was seeded only onto text nodes, not element blocks → `.hero{text-align:center} > h1` reset to Left. Hero (logo/tagline/version) left-aligned. | hero now centers; also lifted article-typography, card-grid, holdout-gradient-text (7.53→4.17) — centered content across the suite |
| **#48** `atlas/canvas-bg-double-paint` | Shorthand dual-stores a gradient in `background_gradient` AND `background_layers`; §14.2 canvas propagation cleared only the legacy field → body's translucent radial glow composited **twice** (α 0.15 → 0.277 = 1−(1−α)²). | body bg matches Chrome within 1/255; **settings 7.11 → 3.98** (same double-paint) |

Board after both: **campaign 24/26 @ t15 avg 7.4**, **holdout 6/6 avg 5.2**.

**Key methodological note:** the diff-attribution tool labels `html>body` as
36% `gradient_interpolation`, and the directive assumed `about` was a
letter-spacing/text-metrics residual. **Both were wrong.** Isolating the body
gradient on a `<div>` proved the gradient interpolation is correct
(within 1/255); the "body 36%" is the corner-ratio heuristic **mislabeling
misplaced-glyph diff** as gradient. The real residual is layout, below.

---

## Remaining residual — three root causes (instrument-confirmed)

Renders: Chrome `baselines/chrome-148/builtins/about/baseline.png` vs RustKit
`parity-baseline/captures/about/frame.ppm` (800×600).

### 1. Shrink-to-fit missing for atomic inlines (BIGGEST lever) — `LAYOUT FEATURE`

The donation button `<a class="sponsor-btn" display:inline-flex>` inside
`<div style="text-align:center">` renders **full-width (x=64 w=672)** in
RustKit vs Chrome's **shrink-pill (x=234 w=332, centered)**.

Falsification fixture (`/scratchpad/iflex.html`, reproduced below): BOTH
`display:inline-flex` AND `display:inline-block` with `width:auto` render
full-width left-aligned in RustKit; Chrome shrink-wraps + centers both.

Root cause: `calculate_block_width` (rustkit-layout/src/lib.rs ~L1953)
resolves `width:auto` to `containing_block.width − MBP` **unconditionally** —
there is no shrink-to-fit branch for atomic inlines. Correct behavior:
`width = min(max-content, max(min-content, available))`.

**Why it's a blocker, not a dig:** shrink-to-fit needs max-content/min-content
intrinsic width, which requires a **two-pass** layout (lay out children, measure
extent, then set width). RustKit block layout is single-pass (width → children).
This is a real feature with broad blast radius (every inline-block/inline-flex
sizing) and should land with an outside-eye review + its own campaign+holdout
gate. Repro:
```html
<div style="text-align:center">
  <a style="display:inline-flex;padding:12px 24px">Support Independent Development</a>
</div>
<!-- RustKit: full-width left. Chrome: 332px pill, centered. inline-block same. -->
```

### 2. Subtitle wraps 2 lines vs Chrome's 1 (advance-width) — `FONT METRICS`

`.tagline` (font-size 1.25rem, font-weight 300) in the 672px hero wraps
"Google." to a 2nd line in RustKit; Chrome fits one line. This shifts the
ENTIRE lower page by one line-height → the heatmap "doubling" of every
paragraph below. Cause: RustKit glyph advances run slightly wide (also visible
as the logo at font-weight 200 rendering heavier than Chrome — light weights
may not be selected/synthesized). Font-shaping territory; measure advance sum
vs Chrome before touching `rustkit-text`.

### 3. `🎯`/`☕`/`✨` emoji not rendered — `FONT/EMOJI`

Color-emoji glyphs in card titles paint blank in RustKit (reserve space, no
ink). Separate emoji-font path; low score weight, do last.

---

## Recommended next order (fresh session, ideally with outside-eye back)

1. **Shrink-to-fit atomic inline width** (#1) — biggest lever, own PR + gates.
   Likely also lifts other pages with centered auto-width inline-blocks.
2. Re-measure `about`; if still >15, attack subtitle advance (#2).
3. Emoji (#3) last.

`about` is honestly parked at 16.49 with the layout half of the diff removed
and a precise map for the structural half. Holdout untouched (6/6).

— Atlas
