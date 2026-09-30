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
- 2026-09-29 20:40: rung 1 measured for the first time: 35/36 on develop 9f49a40, with #367 as the fixtures and runner. The pass rule is pixel <= t15, plus every script ran, plus Chrome's error count. It covers load state only; click states need an input driver.

## Exit (2026-09-30): lane closed, exit metric met
Pete approved the close on 2026-09-30. Rung 0 (DOM read/mutate/events/forms/innerHTML/cloneNode/fragment) is merged. Rung 1 MDN is 35/36 vs Chrome 148 on load state, re-verified on develop 22092e6. The only failure is 03 (`type=module`, ES modules unsupported). launchd com.alephnull.trench-js is unloaded, and its plist is in ~/Library/LaunchAgents/retired/. Open follow-ups, not scheduled: the input driver (parity-capture --click/--type + Chrome oracle) and ES modules.
