# n40 — the emoji line: a glyph the face lacks was shaped as .notdef, and `normal` never saw the fallback face

Seat: Atlas (macOS). Branch `atlas/n40-fallback-face-line-height` from develop `5b89ed8`.
Lane per the n39 digest decision 2 condition: #174/#173/#176 all still open at
lane start → option (b), inline-flex/inline-block vertical sizing, from develop.

## What the n39 table named
about's whole visible residual after #176 was two heights: `a.sponsor-btn`
dh −8 (block path 42, Chrome 50) and `h2.card-title` dh −5.4 (with
`span.icon` dh −6, dw −2.4), and everything under card 3 riding at dy −8/−13.4.
Both elements carry an emoji (`☕`, `🎯`). The n39 read ("inline-flex vertical
sizing / heading line-height term") was wrong about the mechanism: neither
inline-flex nor the heading had anything to do with it.

## Receipt first: pinned Chrome on a 12-row repro
`parity-tests/repro/emoji-line-height.html` via `scratch_n36/chrome_capture.py`:

| row | Chrome | RustKit before | after |
|---|---|---|---|
| 16px system-ui plain | 18 | 18 | 18 |
| 16px + ☕ | **26** | 18 | 26 |
| 20px plain | 23 | 23 | 23 |
| 20px + 🎯 | **29** | 23 | 29 |
| 18px 600 + ✨ | **28** | 21 | 28 |
| 32px + ⚙️ | **42** | 38 | 42 |
| 16px + ☕, `line-height: 24px` | 24 | 24 | 24 |
| inline-flex button (12px pad) + ☕ | **50** | 42 | 50 |
| flex h2 18px 600 + `span.icon` 20px 🎯 | **29** / 29 | 23.55 / 23 | 29 / 29 |
| Arial 16 plain | 18 | 17.52 | 18 |
| Arial 16 + ☕ | **26** | 17.52 | 26 |

CoreText (`scratch_n40/ct_metrics.py`) for the named face "Apple Color Emoji":
16px asc 20.000 / desc 6.250; 18px 21 / 6.56; 20px 22 / 6.875; 32px 32 / 10.
`round(asc) + round(desc)` = 26 / 28 / 29 / 42 — exactly Chrome's four
emoji rows. So Chrome's line box under `line-height: normal` is the union of
the primary face and every fallback face the line used (Blink
`NGInlineBoxState::AccumulateUsedFonts`), each face rounded and carrying its
own half-leading; an explicit `line-height` ignores the used faces.

## The mechanism in RustKit
- `rustkit-layout` `TextShaper::shape` (macOS) shaped the whole run with ONE
  CTFont from the chain. `CTFontGetGlyphsForCharacters` returns glyph 0 for a
  character the face lacks; the run then carried the **.notdef advance**
  (15.69px on SF at 16px — the `advance == 0.0` guard for the half-em
  placeholder never fired either) and the primary face's extents.
- Paint (`rustkit-text` `GlyphRasterizer::rasterize_fallback`) already walked
  a fallback list per character and drew the emoji from Apple Color Emoji at
  its real 21px advance — so the picture showed the emoji overlapping the
  next letter, in a line box 8px too short.
- `normal_line_height` measured `"x"` in the primary face — correct for the
  strut, blind to the run.

## Two traps, both recorded
1. `CTFontCreateForString(systemFont, "☕")` returns `.AppleColorEmojiUI`,
   which reports the **system font's** metrics (15.46 / 3.38 at 16px). Chrome
   (Skia) falls back to the named face "Apple Color Emoji" (20 / 6.25). Use
   the named face; the CoreText cascade lookup is the wrong oracle here.
2. Glyph 0 carries a real advance; the miss is the id, never a zero advance.

## Fix (rustkit-text + rustkit-layout, one PR)
- `rustkit_text::macos::GLYPH_FALLBACK_FAMILIES`: the one fallback list,
  used by paint's `rasterize_fallback` and layout's shaper.
- `shape`: a missing glyph takes the first fallback face that maps it (its
  advance); the run's metrics become the union of primary and used fallback
  faces with per-face half-leading (Arial's rounded 1px gap does not stack
  on the emoji face — 26, not 27). Default-ignorables no face maps (VS16,
  ZWJ…) are zero-width instead of half-em tofu.
- `run_line_height(style, size, metrics)` in rustkit-layout: under `normal`
  the run's united extents win over the style's `normal`; explicit
  line-heights are untouched. `layout_text`, `layout_text_in_flow` and paint's
  `render_text` all derive the line height (and so the half-leading that seats
  the baseline) from it — layout and paint cannot disagree.
- `normal` now rounds the line gap as Blink does (Arial 16px 17.52 → 18).

## What it does not do (ledger)
- Multi-line runs take the union over the whole run for every line; Chrome
  grows only the line that carries the fallback glyph. Per-line heights need
  `TextLine` to carry a height and paint to consume it — not tonight.
- The fallback list is paint's static five; CJK goes to Arial Unicode MS /
  Helvetica Neue where Chrome uses PingFang. Same face as paint, so
  measure == draw; the face choice itself is a separate lane.
- `span.icon` width 23 vs Chrome 22 (+1): Apple Color Emoji advance 21 +
  glyph-0 rounding somewhere in the flex item — not chased.
- The n39 `.command-input-wrapper` 41 vs 43 (shelf) and the ellipsis repro's
  19 vs 18 rows are NOT this family (no fallback glyphs there); still open.
