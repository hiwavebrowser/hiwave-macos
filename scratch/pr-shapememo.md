**B5 (real-site trench): memoise `TextShaper::shape` on macOS.** It covers #277's case (text measurement goes through `shape`) plus line wrapping, which #277 does not reach. If this lands, #277 is redundant; see the note there.

## Why

A symbolized `sample` of cnn on #277's build: 70% of the main thread was in `relayout`, and 29% was line wrapping (`wrap_text_lines` → `wrap_segment` → `CTLineCreateWithAttributedString`). The time was spread over thousands of small shapes from many stacks: every flex/grid measuring pass re-wraps its text, and each wrap shapes one prefix per break opportunity. The same strings get shaped again and again. Font *resolution* was already cached; shaping was not.

I also tried a galloping search in `find_line_break` (O(log n) prefix shapes per line instead of n). It saved only 9% of shapes at about 5 words per line, so I dropped it. Repetition is the cost, not the per-line algorithm.

## What

The macOS `shape()` keeps each `ShapedRun` in a per-thread memo keyed on every argument (text, family chain, weight, style, stretch, size). The memo is dropped whenever the installed `@font-face` set changes, which is the rule `create_ct_font_with_traits` uses. It is bounded at 16384 entries and cleared when full. The uncached body moved unchanged into `shape_uncached`. Windows and the fallback shaper are untouched.

## Tests (each fails without the memo)

- `rewrapping_the_same_text_shapes_nothing_new`: a second wrap of the same paragraph at the same width shapes nothing new and gives identical lines. Bypassed: `93 new shapes re-wrapping the same text`.
- `a_new_web_font_set_invalidates_shaped_runs`: the second shape is a hit, and a web-font install forces a re-shape. It judges the hit only while the process-wide generation holds still, because other tests install font sets on their own threads.

## Real-site board (Chrome 148)

| run | engine | points | loads | readable | looks-right |
|---|---|---|---|---|---|
| `20260926T1100Z-dev` | develop a0176dd | 12/60 | 9 | 2 | 1 |
| `20260926T1245Z-shape` | + this PR | **17/60** | 12 | 4 | 1 |

Rows that moved:
- **netflix 0 → 2:** LOADS and READABLE 82.9%.
- **github 0 → 1:** LOADS.
- **cnn 0 → 1:** LOADS. READABLE is 43.6%.
- linkedin 1 → 2 is **not** this PR. It is oracle variant drift: Chrome showed 57 words in the dev run and 35 afterwards, and RustKit drew 34 in both.
- wikipedia's LOOKS RIGHT was scored unstable in this run (Chrome against Chrome 42.5%). Its READABLE and diff are unchanged (55%, 18.2%).

Standalone wall time, develop → this PR: **cnn 51.8 → 25.7 s, with a byte-identical frame**; github 44.1 → 17.5 s (identical frame); netflix 54.0 → 8.5 s (the live page rotates, so its frames differ run to run on develop too).

## Gates

- layout 518/518, engine 144/144, css 41/41.
- Campaign (`scripts/parity_test.py`): 26/26, avg 1.3%, **every case's diff identical to develop a0176dd**.
- WPT not run: this seat has no `third_party/wpt`.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
