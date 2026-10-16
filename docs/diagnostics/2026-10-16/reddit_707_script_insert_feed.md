# #707 reduced fixture: an inserted chunk chain that outlasts the script budget leaves the feed blank

Measured on develop `c3437a5e` (parity-capture, `--profile parity`). The before-#707 build is `c910f494`, the first parent of the #707 merge `26c46d95`.
Chrome is Chrome for Testing 148.0.7778.216, headless, 800x600, DPR 1.
Measure only: no engine code changes.

## Files
- `reddit_707_script_insert_feed.html`: 17 lines. The page has two server-rendered posts. An inline script inserts `reddit_707_chunk_a.js` (async). That chunk's `onload` inserts `reddit_707_chunk_b.js`.
- `reddit_707_chunk_a.js`: hydration starts. The server-rendered posts are replaced by a skeleton.
- `reddit_707_chunk_b.js`: the feed. An IntersectionObserver callback on a sentinel renders three posts.
- `reddit_707_serve.py`: Python's `http.server` for this folder. A `.js` request with `?delay=<ms>` is held that long, standing in for a slow CDN. The page passes its own `?delay=` (default 3000) to both chunks.

## Run it
```
cd docs/diagnostics/2026-10-16
python3 reddit_707_serve.py <port> &
parity-capture --url "http://127.0.0.1:<port>/reddit_707_script_insert_feed.html?delay=3000" \
  --width 800 --height 600 --timeout-ms 30000 --dump-frame out.ppm --dump-display-list out.json
```
The page writes every step into `#log`, so the display list's text runs list what ran. In Chrome, read `#feed` and the console.

## Results
| engine / build | delay per chunk | script budget | `#feed` | log | script_stats |
|---|---|---|---|---|---|
| Chrome 148 | 0, 3000, 8000 | n/a | 3 feed posts | a inserted, **DOMContentLoaded**, a ran, a load, b inserted, b ran, b load, IO rendered 3 | n/a |
| RustKit c3437a5e | 0 | 5 s (default) | 3 feed posts | a inserted, a ran, a load, b inserted, b ran, b load, IO rendered 3, **DOMContentLoaded** | ran 3 |
| RustKit c3437a5e | **3000** | 5 s (default) | **skeleton only, no posts** | a inserted, a ran, a load, b inserted, **(nothing: no b ran, no load, no error)**, DOMContentLoaded | ran 2, **over_budget 1** |
| RustKit c3437a5e | 3000 | 20 s (`--script-budget-ms 20000`) | 3 feed posts | as Chrome, but DOMContentLoaded comes last | ran 3 |
| RustKit c3437a5e | 6000 | 15 s (the app's content-view budget) | 3 feed posts | as Chrome, but DOMContentLoaded comes last | ran 3 |
| RustKit c3437a5e | **8000** | 15 s (the app's content-view budget) | **skeleton only, no posts** | b inserted, then nothing, DOMContentLoaded | ran 2, **over_budget 1** |
| RustKit c910f494 (before #707) | 3000 | 5 s | **2 server-rendered posts** | a inserted, DOMContentLoaded (inserted scripts never ran) | ran 1 |

## The failing behaviour
Since #707, a script inserted while the page loads is fetched and run inside the load's script budget (`script_net::pump_inserted_scripts`, called from `pump_all` after the page scripts). That budget is `script_budget_ms`, counted from when the page scripts start: 5 s by default, 15 s in the app's content views.

1. **An inserted script still loading when the budget runs out is dropped.** It never runs, and its element hears neither `load` nor `error` (`ScriptOutcome::OverBudget`). Nothing retries it later: it was taken off the queue. A loader that chains chunks through `onload` stops at that point for good, so any later step never happens: here the IntersectionObserver feed. The page is left in its half-hydrated state, a skeleton where the server-rendered posts were. Chrome waits for the network instead, and renders the feed at every delay tested.
2. **DOMContentLoaded and `load` wait for the inserted scripts.** RustKit runs the lifecycle steps (DOMContentLoaded, then `load`, then timers) only after the inserted-script pump returns. A slow chain therefore holds back DOMContentLoaded, and the timers too, for up to the whole budget, and `document.readyState` stays `loading` while the async chunks run. In Chrome, DOMContentLoaded fires before the first async chunk runs.

The chain has to finish within the budget. Raising the budget brings the feed back with the same delays (rows 4 and 5). At delay 0 it always does (row 2).

## Is it #707?
Yes. Before #707 (`c910f494`) inserted scripts never run, so the server-rendered posts stay. That is the same split as the live A/B, where Trace's `TRACE_DIAG_NO_INSERTED` switch, which turns off #707's insertion path, was the only switch that brought Reddit's feed back.
On the live site the app's log fits behaviour 2, though this doesn't prove it. In the control run the page-script phase lasted about 6.5 s, and no lifecycle step (DOMContentLoaded, `load`, timers) was logged before the frames were taken. With insertion turned off, the same phase took 14 ms and the lifecycle followed at once.

Not seen as a cause here: ordering (chunks run in insertion order), `document.currentScript` (it is the element), `onload` firing for a script inserted from inside an `onload` (it fires), and IntersectionObserver (its callback runs). All of these match Chrome when the chain fits in the budget.
