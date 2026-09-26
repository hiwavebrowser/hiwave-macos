**B5 (real-site trench): memoise text measurement.**

## Why

A symbolized `sample` of netflix put 17 s of its first layout pass in Core Text shaping. The calls came through `measure_text_with_spacing` ← `grid::own_min_content_width` / `own_max_content_width`, which `flex::layout_flex_container_in` and `calculate_block_width` call recursively. Intrinsic sizing measures a subtree once for each ancestor that asks, so on a deep flex page every word is shaped dozens of times. Font *resolution* was already cached; shaping was not.

## What

`measure_text_with_spacing` keeps its results in a per-thread memo keyed on every argument (text, family, size, weight, style, letter- and word-spacing). The memo is dropped whenever the installed `@font-face` set changes, which is the same rule `create_ct_font_with_traits` uses. It is bounded at 16384 entries and cleared when full. The uncached body moved unchanged into `shape_text_metrics`.

## Tests (each fails without the memo)

- `a_text_measurement_is_shaped_once_not_once_per_call`: 200 identical measurements shape once and equal the uncached width. Spacing is part of the key. With the memo bypassed: `200 shapes for 200 identical measurements`.
- `a_new_web_font_set_invalidates_text_measurements` (macOS): a web-font install forces a re-shape.

## Real-site board (A/B in one session, alternating 5-site chunks, Chrome 148)

| run | engine | points | loads | readable | looks-right |
|---|---|---|---|---|---|
| `20260926T1100Z-dev` | develop a0176dd | 12/60 | 9 | 2 | 1 |
| `20260926T1100Z-memo` | + this PR | **16/60** | 11 | 4 | 1 |

Rows that moved:
- **netflix 0 → 2:** LOADS (was `capture exceeded 30000 ms`) and READABLE 82.9%.
- **github 0 → 1:** LOADS (was a timeout). READABLE is 63.3%.
- linkedin 1 → 2 is **not** this PR. It is oracle variant drift: Chrome showed 57 words in the dev arm and 35 in the memo arm, while RustKit drew 34 words in both.
- cnn still times out (54.3 → 33.1 s standalone).

Standalone wall time, develop → this PR: netflix 54.0 → 13.5 s, github 44.1 → 19.7 s, cnn 54.3 → 33.1 s. **github's and cnn's frames are byte-identical between the two builds.** netflix's frames differ by 0.7–1.1%, but develop against itself differs by 1.1–3.2% on that live page (rotating content), so that difference is the page, not the memo.

## Gates

- `cargo test -p rustkit-layout -p rustkit-engine -p rustkit-css`: layout 518/518, engine 144/144, css 41/41.
- Campaign (`scripts/parity_test.py`): 26/26, avg 1.3%, **every case's diff identical to develop a0176dd**.
- WPT not run: this seat has no `third_party/wpt`.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
