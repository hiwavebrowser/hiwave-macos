# n33 — the @font-face lane was nine engine gaps, and loading the font was only the first
2026-08-26, macOS trench seat (Atlas). Branch `atlas/webfont-load` → develop.

## Board
| | develop `7591e1c` | `atlas/webfont-load` tip |
|---|---|---|
| WPT Tier-1 scored | 9/25 (36.0%), 1 ERROR | **16/26 (61.5%), 0 ERROR** |
| honest (ex "font never loaded") | 4/25 | 16/26 — the tag no longer means "never loaded" |
| campaign pixel board | 26/26, avg 4.25% | **26/26, avg 4.1%** (23 of 26 cases improve) |

Every number measured on a fresh-built `parity-capture` with the n24 freshness guard on.

## The chain, in the order the receipts forced it
Each step exposed the next; none was visible before the one above it.

1. **@font-face loaded nothing.** Parse (#124) and partitioned `FontLoader` (#128/#129) had
   no middle: `load_font` inserted empty bytes, `load_pending` had zero callers, nothing read
   `Stylesheet::font_face_rules()`. Fix: `rustkit_text::webfonts` registry slot (CGFont from
   bytes → CTFont ahead of every `new_from_name`); loader stores real bytes; engine collects
   rules, resolves against the document base, loads data:/file:/local synchronously and
   http(s) with subresources, installs the view's partition before each layout/paint.
   Security rule: a remote document never reads the local filesystem. Board: 9/25 → 9/25,
   but 15 of 16 fails MOVED (Ahem was finally painting).
2. **z-index never reached the layout box.** Parsed into ComputedStyle; the display-list
   builder already groups negative-z first; `transfer_positioning` never copied it. Every
   `z-index:-1` overlay painted ON TOP of in-flow text. One line. 9/25 → 9/25, family collapses
   (lba004 0.70 → 0.17, wbba010 0.45 → 0.03).
3. **CoreGraphics font smoothing dilated every glyph.** Ahem 20px square rasterized 22
   columns wide with a 60%-coverage row above. Skia/Chrome disable smoothing for grayscale
   AA. 9/25 → 12/25; campaign avg 4.2 → 4.1 with 23/26 cases improving — Chrome's text weight
   was never smoothed, ours was, on every page.
4. **`line-break: anywhere` was aliased to `overflow-wrap: anywhere`.** Different property:
   overflow-wrap only breaks a word that overflows on its own; `anywhere` fills each line to
   the last character that fits. Own `LineBreak` computed value; layout maps it onto break-all
   opportunities. 12/25 → 14/25.
5. **Edge spaces stripped regardless of white-space** (pre/pre-wrap/break-spaces). Fixed —
   and it REGRESSED three passes, which is how gap 6 was found: they were fake passes where a
   stripped space and a no-op `<br>` cancelled.
6. **`<br>` did not exist.** UA display:inline → empty inline with no content children →
   filtered out of the tree. "a<br>b" rendered on one line on every page; it only looked
   right where the preceding text filled the container exactly. New `BoxType::LineBreak`,
   closes the line in both inline-flow loops, empty br line = container line-height.
7. **`white-space: break-spaces` unparsed** (fell to normal).
8. **U+00A0 skipped as break-point whitespace** (`char::is_whitespace` says yes; css-text
   says only document white space collapses).
9. **The `font` shorthand was never parsed.** `font: 20px/1 Ahem` set nothing — the page
   rendered in the inherited 16px fallback. That is why overflow-wrap-001/002 "passed" before
   (both sides wrapped identically in the wrong font) and failed honestly once `<br>` made the
   reference correct; parsing the shorthand passes them for real and un-blanks the ERROR case.
   14/25 → 16/26.

## What the tag means now
`blocked_by: "declares a web font (loads since n33: TTF/OTF; WOFF/WOFF2 not yet)"` is
attribution for trendline continuity only. A fail is measured IN the declared face; a PASS is
real. The runner's "suspect pass" framing is retired in the comment and the console line.

## Ledgered, not chased
- Remaining Tier-1 fails (10): lba001/002 (0.0196/0.0131 — ~90/60 px, not localized),
  lba006 (0.1302 — one 25px cell, NOT the nbsp hypothesis: that fix didn't move it),
  owa002/003 (0.05/0.08), lba005 0.39, owa001/005 2.08, bb2c001 2.25, empty-span-size-002
  0.0102 (outline paint, n21).
- WOFF/WOFF2 — what real sites ship — need a decoder the workspace lacks; today they
  install as nothing and are counted as rejected.
- Relative `src` in an external sheet resolves against the document URL, not the sheet URL.
- The renderer's `TextShaper::new(family, size)` path resolves weight 400 for web fonts.
- `<br>` inside an inline (`<span>a<br>b</span>`) is not handled — only direct children of a
  block's inline flow.
- Preserved spaces under pre-wrap are still skipped at a soft break by the wrapper (it does
  not know white-space).
- The campaign board did not move for `<br>`, the shorthand, or the white-space fixes: no
  campaign page exercises them where Chrome differs. Real sites will.

## Receipts
Commits on `atlas/webfont-load`: `3513588` (fonts), `ca46b9e` (runner wording + receipts),
`517cfdd` (z-index), `d480663` (smoothing), `c04f578` (line-break), `72ebf2c` (br +
`font` shorthand + white-space + nbsp). Scratch: `scratch_n33/` (probe scripts, repros,
captures).
