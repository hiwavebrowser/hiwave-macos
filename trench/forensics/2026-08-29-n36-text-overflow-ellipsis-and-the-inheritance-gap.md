# n36 — text-overflow: ellipsis, and the inheritance gap it uncovered
2026-08-29, macOS trench seat (Atlas). Branch `atlas/n36-text-overflow-ellipsis` → develop.

## Board
| | develop `2be7d37` (fresh build, reproduced) | this branch @ 803d62a |
|---|---|---|
| WPT Tier-1 scored | 24/26 (92.3%), 0 ERROR | **24/26 (92.3%), 0 ERROR** — unchanged |
| campaign pixel board | 26/26, avg 4.0546% | **26/26, avg 4.0546%** — 0 of 26 moved, byte-flat |

Every number on a fresh-built `parity-capture`. Suites: rustkit-css 27,
rustkit-layout 305, rustkit-engine 69, rustkit-renderer 63.

**The meter does not see this lane.** No campaign case carries a truncated
title (the chrome_rustkit tab titles fit; settings' cellar list is empty in
the capture), and no Tier-1 seed case declares `text-overflow`. Receipts are
therefore unit tests + a pixel A/B against pinned Chrome on a repro, not the
board. That is the same shape as #165.

## The lane as taken (n35 decision 2, option a)
`text-overflow` did not exist: no value type in rustkit-css, no arm in the
engine's property switch, no notion in paint of where a line box ends. n35
made `overflow: hidden` clip text for real, which cut every
`.tab-title` / `.url-text` / `.command-item-name` / `.cellar-item-*` hard at
its box; this is the visible half those rules asked for.

## Where the ellipsis lives — paint, not layout
css-overflow-3 §5.1 is stated on the block container's line boxes. The
display-list builder already owns the block's clip and every run's per-char
advances (the ADVANCE CONTRACT), while layout's single-line nowrap path
clamps a text box's `content.width` to its container — so the true ink
extent of an overflowing run is only known where `render_text` shapes it.

- Entering a `BoxType::Block` replaces the builder's `EllipsisScope`: an
  ellipsis scope at the block's **content** end edge when the block clips
  and asks for one, none otherwise; restored after the subtree. Inline,
  anonymous and text boxes paint inside the nearest block's scope.
- A run whose advances overrun the edge is cut so `…` (shaped in the run's
  font) fits inside it. If not even the ellipsis fits, the ellipsis alone
  paints — §5.1 says clip it, not skip it. Decorations use the cut width.
- Once a line is cut, every later run reaching that line is hidden — by
  inline ORDER, not by x. The first version compared x against the
  ellipsis and the repro's mixed-inline row still painted " th…" over the
  span's ellipsis. That led to the second finding.

## The second finding: five inherited properties stopped at the first element
The repro's `Inline <span style="font-weight: bold">bold run that
overflows</span> the box` laid the span's text out **34px tall** under
`white-space: nowrap` — two lines, the second clipped — and placed " the
box" on line one at the span's clamped end. The ellipsis logic did what
layout told it.

`compute_style_for_element` seeded font-family/size/weight/style/stretch,
color, letter/word-spacing and text-align from the parent and stopped. Its
comment said white-space was "handled separately". It was not: the only
other site is the text-node copy, which reads its OWN element's value, so
`.title { white-space: nowrap }` reached bare text and never a `<span>`
inside it. `parity-tests/repro/nowrap-span-probe.html`: bare span, style
attribute span and class span all wrapped; only a span restating nowrap
itself did not. Same for `word-break`, `overflow-wrap`, `line-break`,
`text-transform` — all css-text-3 inherited properties. Fixed by seeding
the five (`803d62a`); an author value on the child still wins.

Real-page reach of the second fix is wider than the first: any inline
element inside a nowrap/pre/break-all/uppercase container now behaves as
its container says. The campaign board did not move because its pages set
these properties on the text-carrying element itself.

## A/B vs pinned Chrome (parity-tests/repro/text-overflow-ellipsis.html, 400×220)
| row | Chrome 148 | RustKit @ 803d62a |
|---|---|---|
| flex `.title` (the chrome tab shape) | The quick brown fox jumps o… | The quick brown fox jumps o… |
| `.plain` block | The quick brown fox ju… | The quick brown fox ju… |
| `.plain` with bold span | Inline **bold run that ov…** | Inline **bold run that ov…** (was "bold run that  th…" before `803d62a`) |
| clip only | cut hard, no ellipsis | same |
| fits | whole | whole |
| overflow: visible + ellipsis | overflows, no ellipsis | same |

The remaining per-row pixel diff is the pre-existing `line-height: normal`
family (RustKit rows 19px tall vs Chrome 18px at 14px sans-serif) and
glyph-advance width, not this lane.

## Ledgered, not chased
- The ellipsis is shaped in the RUN's font; §5.1 says the block
  container's. Identical whenever the run inherits the block's font.
- Layout still clamps a nowrap run's `content.width` to its container, so
  the sibling after an overflowing span sits short of its true x. Paint
  hides it (order rule); layout geometry for such a sibling is still wrong.
- `<pre>` has no UA `white-space: pre` default in the engine (only the
  property arm sets Pre). Not measured on any current case.
- Two-value `text-overflow` and the `<string>` form parse as ellipsis-if-
  present.
- Remaining Tier-1 fails unchanged: line-break-anywhere-001 0.0173 (AA
  column, tolerance decision since n33), empty-span-size-002 0.0121
  (outline paint).

## Receipts
Commits on `atlas/n36-text-overflow-ellipsis`: `a9881b0` (text-overflow,
3 crates + 8 tests + repro), `803d62a` (five inherited properties + test
+ probe), board receipts, this note. Scratch: `scratch_n36/` (`rules.py`
CSS-rule finder, `chrome_capture.py` ad-hoc pinned-Chrome capture via the
parity oracle, `ppm2png.py`, `rowdiff.py`, `waitbuild.py`, the three
RustKit frames and the Chrome baseline).
