# JS-ladder trench — BASELINE

end_date: 2026-09-30
exit_metric: js-ladder rung 0 read slice + mutation surface merged, and rung 1 (MDN learning-area DOM set) >= 80% passing vs Chrome 148
max_open_prs: 3
pr_repo: hiwavebrowser/hiwave-macos
pr_head_prefix: atlas/js-

## Why this lane exists
Pete spent the one-time limits reset on 2026-09-28 and said "burn hot till Wednesday night". This lane takes the JS track off the real-site lane's queue so it runs in parallel. end_date is Wednesday: after the weekly reset the budget drops, and this lane is the first one to stop. Extend it only with Pete's OK.

## Metric
js-ladder: rung N, X/Y fixtures passing (vs pinned Chrome 148), plus WPT dom/nodes pass count once rung 2 starts.

Starting value: rung 0, nothing built. rustkit-bindings serves a pure-JS stub document: getElementById hits an empty map and querySelector always returns null.

## Changes
- 2026-09-28: lane created.
