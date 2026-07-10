## What

Two rustkit-layout inline-flow bugs, found via the per-element y-table method on the backgrounds page (Chrome layout-rects.json vs RustKit layout.json, first-divergence discipline from the session-10 addendum).

### Bug 1 — decorated inline-blocks positioned border+padding up-left

The inline-flow position override placed the **content rect** at the margin-box cursor (`container + cursor + margin.left`), dropping `border.left + padding.left`. The border box then landed at `-(border+padding)` from Chrome's position:

- 2px-border test boxes: −2px in x and y (every box on the page)
- backgrounds test3 row (10px border + 20px padding): −30px, pushing the first box to x=0

Fix: content rect sits `margin + border + padding` inside the cursor, in both `layout_block_children` and `layout_block_children_with_collapse`. This also re-aligns the box with its already-correctly-positioned descendants.

### Bug 2 — line boxes missing the strut descent below empty atomic inlines

Lines of inline-blocks advanced by max margin-box height only (120px rows). Per CSS2 §10.8.1, an atomic inline with **no in-flow line boxes** has its baseline at the bottom margin edge, so the line-box strut's descent + half-leading extends BELOW it — Chrome renders 126px rows. The missing 6px/row accumulated to −68px of vertical drift by page bottom, dragging every section across the striped checker band (this was the whole "vertical drift" driver from the pre-dig, addendum 2026-07-10).

Fix: track `line_below_baseline = max(margin_height + strut_descent)` over boxes whose baseline is the bottom margin edge (`baseline_is_bottom_edge()`: images, and atomic inlines with no children); each line close advances by `max(line_height, line_below_baseline)`.

**Deliberately conditional:** atomic inlines WITH in-flow content keep their internal baseline — their below-baseline part is already inside their margin height. Unconditional strut-adding would have over-advanced content-filled inline-blocks (gradient-backgrounds pills, shelf header) that currently pass. Known approximation, noted in code: an atomic inline whose children are all block-level (still no line boxes) is treated as having an internal baseline; rare in the suite, conservative = current behavior.

## Measurement (pinned CfT 148, t15, two identical runs)

- **Unified pass rate: 20/26 → 21/26 (80.8%) — campaign high**
- **Avg diff: 13.3 → 11.9 — campaign-best**
- backgrounds **27.31 → 12.98 PASS** (the dig target)
- bg-solid → 1.42, gpu-gradient-regression → 8.26, rounded-corners → 5.72
- Zero regressions: every previously passing case still passes; settings/css-selectors/shelf/sticky-scroll/image-gallery statistically unmoved
- Post-fix y-table: all x-positions match Chrome exactly; residual drift +1.7px/row (our `inline_strut_descent()` ≈7.7px vs Chrome's effective 6.0px — font-metric delta, ledgered as a follow-up, NOT chased here)

## Tests

- 235 rustkit-layout tests pass. New regression test `test_inline_block_border_box_position_and_line_strut` covers both bugs.
- `test_inline_flex_children_share_a_line` height expectation updated: a line of empty 40px atomic inlines is now `40 + strut_descent` tall, matching Chrome's rendering of the same markup (the old 40px expectation encoded bug 2).
- `intrinsic_cache` flake pre-exists on clean master (parallelism; passes with `--test-threads=1`), ledgered.
