# STRESS-SPEED: where NewsSite-Nuxt's and NewsSite-Next's time goes (2026-11-24)

Row STRESS-SPEED, the NewsSite pair. On the Mac develop record (truth-1123, frames on), NewsSite-Nuxt
takes 7,496 ms against Chrome 148's 38.5 ms (×195), and NewsSite-Next 7,760 ms against 46.8 (×166).
This is the breakdown, measured before any change, using only the engine's own timing.

## Short answer

On legion5, both workloads take about 12.2 s (median of 3). **About 80% of that is forced layout
after a checkbox toggle**, and most of the layout time is **flex**.

1. **Forced layouts: about 9.8 s (81%).** Each NewsSite step toggles the navbar dropdown checkbox
   (`#navbar-dropdown-toggle`) 50 times, with `page.layout()` after every click (Speedometer's own
   test, below). The dropdown opens and closes through CSS, so every layout is genuine. Each one
   rebuilds and re-lays out the whole frame document (843 elements) in about 62 ms: build about
   10 ms, layout about 53 ms. There are 158 (Nuxt) and 156 (Next) of them. Chrome does the same
   work in well under a millisecond each.
2. **Inside layout, flex: about 7.7 s inclusive.** One pass lays out at most about 330 distinct
   flex containers but makes 1,100–1,470 flex-container layouts: each is laid out about 3.5–4.5
   times per pass, at about 40 µs each.
3. **Box tree build: about 1.5 s** (about 10 ms per forced layout).
4. **Script, host calls excluded: about 1.6 s** (Nuxt) and **2.3 s** (Next).
5. **Grid: about 1.4 s inclusive**, and **text measurement: 8.7 million memo lookups, about
   1.0 s**. They are all hits (only about 1,500 shapes in a run). Each lookup builds a key that
   copies the text and the family name into two new `String`s.

Rows 2 and 5 are inclusive, nested timers inside row 1, so they overlap it and each other: they say
where layout's time goes, not extra time.

**Rows 1–5 are not the mechanism of #1446, #1447 or #1448** (the window event path, named access,
the SVG paint-rule cache). Host calls total only about 0.45 s. The largest rows are geometry reads
(`__rustkit_dom_info/geom`, 1,604 calls, 201 ms) and `__rustkit_dom_collect` (1,625 calls, 172 ms).
`__rustkit_dom_collect` is the host function that also carries #1447's named-access arm, and the
log doesn't split it by kind. Whatever named-access share it has is inside those 172 ms, and it's
Athena's.

## Speedometer's step (pinned 3.1, `resources/tests.mjs`)

```js
new BenchmarkTestStep("NavigateToUS", (page) => {
    for (let i = 0; i < 25; i++) {
        page.querySelector("#navbar-dropdown-toggle").click();
        page.layout();
        page.querySelector("#navbar-dropdown-toggle").click();
        page.layout();
    }
    page.querySelector("#navbar-navlist-us-link").click();
    page.layout();
}),
```

`NavigateToWorld` and `NavigateToPolitics` are the same. Each step gives 50 form writes (the
checkbox's `checked`) and 51 forced layouts. The engine's forced-read log names them:
`why=a write a layout can see is pending read="rect" cause="form"` (149 of 158 in Nuxt).

## How it was measured

- **Binary:** `parity-capture`, release build of macOS develop `b3329bff`, built and run on legion5
  (Linux, `HIWAVE_NESTED_CONTEXTS=1`, no display, CI fonts).
- **Tool:** `tools/stress/run.py` from #1445 (`athena/stress-windows` `bbda2378`), so the records
  are valid on Linux. Command: `--suite speedometer --engine hiwave-headless --only <workload>
  --iterations 1 --frame-stages --keep`. Speedometer is pinned to `1386415b`; everything is served
  from 127.0.0.1.
- **Validity:** every record says `load_valid: true` and `off_machine: {}`, with 15 socket samples
  and no failures, and the engine log's URLs checked. One run at a time, load average about 3.
- **Where the numbers come from:**
  - The workload time is Speedometer's own value (`sub_scores`).
  - Stage totals are the sums of the engine's `Frame stages` lines.
  - Forced layouts are the engine's `Forced read builds the tree` lines (count, `took_ms`,
    `layout_ms`).
  - Rows 2 and 5 come from one-off, uncommitted inclusive timers (time and calls) at
    `measure_text_with_spacing`, `shape_text_metrics`, `layout_text`, `layout_inline`,
    `layout_flex_container_in` and `layout_grid_container`, printed and reset at each forced
    layout. Those were two separate runs ("probe", "probe2"), not the three baseline runs, and the
    timers are removed. The distinct-container count is an upper bound: it counts addresses, so a
    box laid out as a clone counts again.

## Baseline (median of 3, ms)

| | NewsSite-Nuxt | NewsSite-Next |
|---|---:|---:|
| workload (Speedometer's value) | 12,167 (12,255 / 12,167 / 11,661) | 12,210 (12,327 / 12,210 / 11,961) |
| forced layouts (count) | 158 | 156 |
| forced layouts (time) | 9,877 | 9,792 |
| of which layout | 8,326 | 8,313 |
| busy, all stages | 13,627 | 13,333 |
| layout stage | 8,807 | 8,591 |
| build stage | 1,580 | 1,468 |
| script stage (host calls included) | 1,641 | 2,321 |
| host calls | 453 | 428 |
| other | 1,483 | 848 |

Records: legion5 `.talos-wt/stress_runs/base/` (six `speedometer_hiwave-headless_devb332_*` records,
with `--keep` work directories).

## Proposed first fix

**FLEX-RELAYOUT.** Lay each flex container out once per pass, not 3.5–4.5 times. Find which
`flex.rs` path lays an item's subtree out again (the line-break, cross-size and final passes all
call into child layout), and reuse a box's result when it is asked for the same available size
twice in one pass. Red first: a test that counts layouts of a nested flex container in one pass
(today about 4, target 1), with Chrome 148 geometry pins on the same markup so the cache can't
change an answer. Expected effect: up to about 70% of row 2's 7.7 s if the repeat factor goes to 1.
That's an estimate to check, not a promise.

**A smaller, safe win** to take alongside it or first: make the text-measure memo look up by
borrowed text and family name (no `String` copies per lookup). That's 8.7 million lookups, about
1 s inclusive, and the answers can't change.

Making the forced layout itself incremental (lay out only what the toggle changed) would remove
most of row 1. That's the incremental-layout plan's ground (`docs/design/INCREMENTAL_LAYOUT_PLAN.md`),
not a first fix.
