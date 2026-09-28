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

## Starting value (measured 2026-09-27 22:25, develop f262568 + the timer from atlas/cs-cascade-timer)

`python3 trench/tools/cascade_bench.py --capture <release parity-capture> --runs 5`

| site | Chrome style ms | RustKit cascade ms (median of 5) | builds/load | ratio | raw cascade ms |
|------|-----------------|----------------------------------|-------------|-------|----------------|
| cnn | 210 | 5852 | 2 | 27.9× | 5524, 6811, 5036, 5852, 5978 |
| github | 110 | 7441 | 3 | 67.6× | 7066, 7441, 7493, 7397, 7452 |
| wikipedia | 20 | 1613 | 3 | **80.6×** | 1615, 1610, 1633, 1613, 1581 |

Worst ratio at start: **80.6× (wikipedia)**. The old ~70× came from log-line gaps on live loads; this is the instrument's number.
Noise: a single wikipedia run 10 minutes earlier read 1133 ms (the other lane was building). Compare medians taken back to back, same session.

## Instrument (frozen by phase 0)

- **Pages:** `trench/cascade/snapshots/{cnn,github,wikipedia}/`, pinned 2026-09-27 by `trench/tools/cascade_snapshot.py`. HTML plus every `<link rel=stylesheet>` (served locally, loaded as external sheets). `<script>` removed: page JS runs against a stub document today, so it can't change the DOM the cascade sees. Images and fonts still resolve to their origins. The page bodies are gitignored: this repo is public, and they are third-party content. Only the manifests and `trench/cascade/PINS.sha256` are committed, so the pages live only on this Mac's hub worktree. Verify with `shasum -a 256 -c PINS.sha256` from `trench/cascade/`.
- **RustKit number:** `RUSTKIT_CASCADE_TIMING=1` makes the engine log `Cascade timing parse_ms=… cascade_ms=…` once per layout build (`build_layout_from_document`). `cascade_ms` runs from after `<style>` extraction to the finished box tree: rule index, custom properties, per-element cascade, box construction. That is the counterpart of Chrome's `UpdateLayoutTree` self time, which is style recalc plus layout-tree rebuild with sheet parsing excluded. The metric is the **sum over every build in one load**, median of 5.
- **Chrome number:** fixed at the ground-truth analysis values above (CfT 148, `style` = UpdateLayoutTree + RecalculateStyles self time, live pages).
- Known asymmetry: the Chrome numbers are from live pages with JS running (Chrome did hundreds of small recalcs), while RustKit's are from pinned, script-free pages. Both measure a page load's total style work.

## Measurement rules (frozen once phase 0 records them)

- The pinned snapshots in `trench/cascade/snapshots/`. Never a live fetch of the document or sheets. (The real-site board turned out to have no snapshots: it loads live.)
- Chrome side: CfT 148, the numbers already in the ground-truth analysis unless phase 0 re-derives them with the same tool (`chrome_groundtruth.mjs` on the realsite hub).
- RustKit side: release build, 1280×800. Record the **median of 5** runs. The real-site lane may be building at the same time on this Mac, so record the 5 raw numbers too.
- Changing the page set, the Chrome build or the method needs a dated line under **Changes** and Pete's OK.

## Changes
- 2026-09-27: lane created (Pete approved a second trench lane after spending the one-time limits reset).
- 2026-09-27 22:25: phase 0. The measurement rule said to use "the real-site board's pinned snapshots", but none exist, so this lane pinned its own (see Instrument). Method frozen. Needs Pete's OK retroactively (digest decision).
