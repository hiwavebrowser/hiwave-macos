# STRESS-SPEED: where Editor-TipTap's time goes (2026-11-24)

Speedometer 3.1's Editor-TipTap. On the Mac record (truth-1123, frames on) it takes 14,465 ms
against Chrome 148's 48.3 ms (x300): the largest absolute time of the 20 workloads, and the
second-worst ratio. Measured before any change, with the engine's own timing.

## Short answer

On Windows the workload takes **11,229 ms** (median of 3: 11,334 / 11,229 / 11,218; develop
`b3329bff`, release, frames on). **Layout is 10.7 s of it.** Script is 0.24 s and host calls are
8 ms.

The frame is laid out in full four times: after a sheet change, and three times after script
forces a layout by reading geometry. Each full layout takes about 2.65 s. The document is
`longtext.html`: 270 KB of prose (Proust, in French) in 196 paragraphs, laid out by ProseMirror
inside two nested flex boxes.

Three costs multiply together:

1. **Wrapping a long paragraph cost the square of its length.** For every line, `wrap_segment`
   shaped the whole rest of the paragraph to ask whether it fits, and `find_line_break` listed the
   break opportunities of the whole rest. One paragraph's full layout, Windows release:

   | paragraph | develop | WRAP-WINDOW |
   |---:|---:|---:|
   | 5,000 chars | 26.6 ms | 18.0 ms |
   | 10,000 | 76.5 | 35.2 |
   | 20,000 | 237.0 | 68.4 |
   | 40,000 | 731.9 | **134.2** |
   | 8 x 5,000 | 198.1 | 127.9 |

2. **Each flex level lays the content out again.** The whole document under the editor page's own
   CSS (`style.css`: `body { display: flex }`, then `#app` a column flex box, then `#editor` with
   `flex: 1`), at 1280x800, with WRAP-WINDOW applied:

   | structure | one full layout |
   |---|---:|
   | the editor page as written (two flex levels) | 2,323 ms |
   | `body` not a flex box (one level) | 1,508 ms |
   | neither a flex box | 779 ms |

   Each level about doubles the cost: a flex item's text is laid out again for every measuring
   pass.

3. **The text layout itself is about 3 us per character, even with no flex.** The whole 232k-character
   document in a plain 780px block takes 691 ms (822 ms on develop). Under `white-space: pre-wrap`,
   which ProseMirror sets, it takes 450 ms on both arms.

Incremental layout (`RUSTKIT_INCREMENTAL_LAYOUT=1`, off by default) skips the relayouts that
follow a small change (2.7 s down to 0.2 ms each) and takes the workload from 11.2 to 8.6 s. The
forced reads still lay the whole tree out, though: "Forced read builds the tree ... read=rect".
The layout memo and tree reuse change nothing here.

## Fixes, in order

1. **WRAP-WINDOW** (this PR). Each line shapes a window of the rest that is sure to overflow it:
   512 characters, doubling, until it passes twice the line plus four ems. The break search runs
   inside that window. Paragraph cost becomes linear (above). TipTap goes from **11,229 to
   8,756 ms (-22%)**, median of 3. The board is identical on both arms.
2. **FLEX-REMEASURE.** Lay a flex item's content out once per available width, not once per
   measuring pass: up to another ~3x on this page.
3. **TEXT-PER-CHAR.** About 3 us per character for wrapping prose: shaping prefixes once per break
   opportunity, and the window shaped per line. Measure it after 2.
4. **Forced reads.** These lay out the whole tree even when incremental layout would do. Also the
   INC5 default question (Atlas's plan item).

## How it was measured

- `tools/stress/run.py --suite speedometer --only Editor-TipTap --iterations 1 --frame-stages`,
  `HIWAVE_NESTED_CONTEXTS=1`, the portable `run.py` (#1445). Records are `status: ok`.
- Per relayout: the engine's `relayout cause=... took_ms= layout_ms= tree_reused= would_path=`
  lines for the frame's view.
- The page tables come from `parity-capture --html-file` on reduced pages built from
  `longtext.html` and `style.css`, reading the initial relayout's `layout_ms`. That is one load
  each, so the numbers are indicative.
