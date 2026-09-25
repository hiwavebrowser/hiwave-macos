# Real-site board analysis scripts

Used for `trench/ANALYSIS-realsite-2026-09-25.md`. Run data is not committed; these
read the local `trench/realsite/runs/` (override with `RS_RUNS`) and write JSON to `RS_OUT` (default `.`).

| Script | What |
|---|---|
| `trend.py` | Every run dir → `trend.json` (per-site checks, ratios, diffs, timings, access, script stats). |
| `matrix.py` | Full-board runs only (≥20 sites): per-site check history + LOADS flip counts. |
| `timeline.py <run> [run…]` | Parses each site's `rustkit-stderr.log` into phase buckets (net:doc/css/font/img, cascade, layout, script) → `timeline.json`. Log-event intervals, so overlapping fetches are approximate. |
| `anatomy.py <run> <site…>` | Chrome-vs-RustKit frame anatomy: 8×5 tile classes (match / missing / extra / differs), vertical shift, display-list op mix, full-viewport fills. |
| `join.py <probe.json> [lib.rs]` | Joins `../structure_probe.mjs` output with the CSS properties `apply_style_property` parses in this checkout. |

Chrome side: `node trench/tools/structure_probe.mjs <out.json> [site…]` with `PARITY_CHROME_PATH` set.
