# Cascade-speed trench — BASELINE

end_date: 2026-10-11
exit_metric: worst style ratio (RustKit cascade / Chrome style) across cnn, github, wikipedia <= 3.0
max_open_prs: 3
pr_repo: hiwavebrowser/hiwave-macos
pr_head_prefix: atlas/cs-

## The one metric

**Worst style ratio** = max over {cnn, github, wikipedia} of
(RustKit total style-cascade time for one page load) / (Chrome 148 style-recalc time for the same pinned page).

This is the finish-line gate from MISSION.md: "cascade within 2–3× of Chrome".

## Starting value (from ANALYSIS-chrome-groundtruth-2026-09-26.md; phase 0 re-measures)

| site | Chrome style ms | RustKit cascade s | ratio |
|------|-----------------|-------------------|-------|
| cnn | 210 | 9.7 | ~46× |
| github | 110 | 6.4 | ~58× |
| wikipedia | 20 | 1.4 | ~70× |

Worst ratio at start: **~70×**. Phase 0 replaces this row with a measured one on develop f262568 or later.

## Measurement rules (frozen once phase 0 records them)

- Same pinned page snapshots the real-site board uses (see atlas/trench-realsite `trench/realsite/`). Never a live fetch.
- Chrome side: CfT 148, the numbers already in the ground-truth analysis unless phase 0 re-derives them with the same tool (`chrome_groundtruth.mjs` on the realsite hub).
- RustKit side: release build, 1280×800. Record the **median of 5** runs. The real-site lane may be building at the same time on this Mac, so record the 5 raw numbers too.
- Changing the page set, the Chrome build or the method needs a dated line under **Changes** and Pete's OK.

## Changes
- 2026-09-27: lane created (Pete approved a second trench lane after spending the one-time limits reset).
