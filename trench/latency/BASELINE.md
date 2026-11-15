# Trench baseline — latency (LAT0)

end_date: 2026-11-20
exit_metric: script-budget stops per minute of interaction <= 1 on the 19-site list AND content-thread hold p95 <= 50 ms

**Row:** LAT0 (hub `trench/z/PLAN-z.md`). **Asked by:** Pete 2026-11-05 ("a few of the sites the logs just churn and churn, and the sites are laggy").
**Source:** existing logs only, no new loads. Reproduce any number here with
`tools/latency/baseline_from_logs.py LOG... [--rank] [--json] [--views]`.

**Note on the exit metric (read before adopting it).** Two findings change what it measures:

1. On the flag-off logs, live budget stops are already at or under 1 per minute everywhere but
   cnn (1.7). On tonight's flag-on develop log they are not: amazon 6.0/min (four ~5 s timer and
   network turns cut by the watchdog), walmart 1.8, ebay 1.3, cnn 1.1. Of the
   1,312 `Script stopped at the script budget` lines in the flag-off hand-test logs, **1,303 are
   input refused by a document the window was leaving** (elapsed 0, between a navigation signal
   and the next document's first script). That is the mislabel `leaving_input_tests.rs`
   describes, fixed on develop by #892 (00:33 11-05) after every one of these logs was taken.
   The tool counts them apart as `leaving`. Tonight's log has **0** leaving stops, so #892 holds.
   What the stops half measures now is real watchdog cuts of 0.5–15 s (mechanism 6, LAT10).
2. Holds are logged only at 50 ms or more (`wgate::HOLD_THRESHOLD_MS`), so the logs can't give
   a p95 over all entries. **LAT1 below adds a per-visit hold summary line** (n over all entries,
   p50/p95, like the `Input to present … n=100` line). Until it lands, the stand-in is
   "logged holds per minute and their p95" plus the existing `Input to present` p95.

Proposed amendment for Atlas/Pete to decide: `exit_metric: on the 19-site list, live
script-budget stops/min <= 1 AND content-thread hold p95 (all entries, LAT1 line) <= 50 ms AND
Input to present p95 <= 100 ms`.

## Sources read

| Source | What it is | Chrome |
|---|---|---|
| `live-20261031T1442.log` | Pete hand test, 466 s of dwell, 7 hosts | flag off (WebKit chrome) |
| `live-20261103T1704.log` | Pete hand test, 1,014 s, 11 hosts | flag off |
| `live-20261104T2116.log` | Pete hand test on 209f6a64, 567 s, 8 hosts | flag off |
| `live-20261105T2246.log` | **Pete hand test tonight, `HIWAVE_RUSTKIT_CHROME=1`**, 03:47:51–03:53:22 UTC (331 s, 47,533 lines, complete); 6 hosts | **flag on** |
| `batch25/x-batch25/*.stderr.txt` | 76 headless loads, the 19-site A/B list x4 (load phase only, no input) | n/a |
| `batch2[345]/rw-on*`, `rw-off*` `*.app.log` | real-window checks on local pages, flag on vs off | both |

`live-20261105T2245.log` is compiler output only (no app start). batch23/24 have no capture
dirs; batch26 had not produced an A/B yet. No `*.events.jsonl` exists in any batch dir.
All three flag-off hand-test logs predate #892 (leaving input), #894 (S22 hover restyle reach)
and the S13 animation package, so they are the **before** for those fixes as well as for LAT.

Definitions: *dwell* = time from a document's first script/relayout to the next navigation
signal. *Visit* = one `Loading URL` for the host. Sites are hostnames only. Holds are the
`Content thread hold` lines (>= 50 ms only).

## 1. Per site, flag-off hand tests (three logs combined)

`stops` = live / leaving. `holds` = logged holds n, ms p50/p95/max. `nav` = `Navigation finished` elapsed_ms p50/max. `scripts` = `Page scripts ran` elapsed_ms p50/max.

| site | visits | dwell s | stops live/leaving | live stops/min | relayouts n, ms | forced layouts | holds n, p50/p95/max | img drops | timer relayouts/s | scripts ms | nav ms |
|---|---|---|---|---|---|---|---|---|---|---|---|
| www.ebay.com | 3 | 328 | 5 / 1 | 0.91 | 251, 134,777 | 89 | 344, 174/2,203/26,459 | 143 | 0.08 | 10,191/10,606 | 25,952/52,309 |
| www.reddit.com | 9 | 378 | 1 / 1,246 | 0.16 | 159, 47,455 | 27 | 155, 432/1,317/13,117 | 2 | 0.02 | 0/11,690 | 450/19,907 |
| en.wikipedia.org | 1 | 128 | 0 / 0 | 0 | 49, 11,431 | 28 | 51, 852/1,318/5,809 | 0 | 0.23 | 18 | 9,947 |
| www.foxnews.com | 1 | 82 | 0 / 0 | 0 | 103, 15,169 | 55 | 107, 149/806/4,163 | 0 | 0.76 | 220 | 14,351 |
| www.walmart.com | 5 | 209 | 0 / 56 | 0 | 123, 10,706 | 0 | 58, 167/6,061/9,251 | 0 | 0.01 | 1,172/1,285 | 6,449/9,448 |
| www.yahoo.com | 3 | 99 | 1 / 0 | 0.60 | 238, 9,440 | 9 | 245, 122/1,371/5,945 | 0 | 1.59 | 386/426 | 5,347/5,945 |
| www.cnn.com | 1 | 71 | 2 / 0 | 1.70 | 21, 5,808 | 62 | 23, 312/5,123/26,136 | 0 | 0.10 | 15,132 | 36,702 |
| www.google.com | 2 | 115 | 0 / 0 | 0 | 185, 5,540 | 27 | 32, 94/2,687/9,867 | 0 | 0.29 | 879 | 13,200 |
| www.apple.com | 1 | 68 | 0 / 0 | 0 | 71, 3,186 | 53 | 55, 80/402/8,068 | 0 | 0.29 | 1,119 | 8,068 |
| www.wikipedia.org | 2 | 14 | 0 / 0 | 0 | 51, 2,061 | 4 | 12, 52/2,024/2,024 | 0 | 0.50 | 39 | 2,889 |
| www.x.com | 1 | 50 | 0 / 0 | 0 | 15, 220 | 0 | 5, 120/9,659/9,659 | 0 | 0.16 | 2 | 9,658 |
| www.amazon.com | 3 | 337 | 0 / 0 | 0 | 4, 6 | 0 | 23, 120/930/1,042 | 0 | 0 | 187/190 | 930/1,041 |
| www.chrono24.com | 2 | 85 | 0 / 0 | 0 | 8, 6 | 0 | 4, 193/540/540 | 0 | 0 | 0 | 1,045 |
| www.gmail.com | 1 | 24 | 0 / 0 | 0 | 4, 166 | 0 | 1, 12,599 | 0 | 0 | 44 | 12,610 |

Budget stops by event source (all logs): pointermove/mousemove 386, pointerenter/mouseenter 332,
pointerleave/mouseleave 321, over/out 166, down/up/click 10 — **every pointer one is a leaving
stop**. Live stops: timers 2 (5,269 and 5,669 ms elapsed), network 3 (1,071; 5,004; 5,004),
page script 3 (5,004; 15,089 inline on cnn; one 0 ms on reddit), DOMContentLoaded 1 (380 ms on
cnn). `late_ms` is 0 on every line.

Holds and hover relayouts by site (the lag Pete feels):

| site | move holds n (/min), p50/p95 ms | live_turn holds n, p50/p95 | hover relayouts n (/min), took p50/p95 | hover split p50: sheet-extract / build / layout | Input to present p95 per 100 inputs, ms |
|---|---|---|---|---|---|
| www.ebay.com | 200 (36.6), 349/975 | 44, 974/5,397 | 199 (36.4), 853/903 | 603 / 197 / 44 | 11,352 … 32,162 (10 lines) |
| www.reddit.com | 104 (16.5), 434/487 | 22, 91/1,895 | 125 (19.9), 420/436 | 4 / 391 / 26 | 6,534; 8,125 |
| en.wikipedia.org | 10 (4.7), 219/221 | 30, 864/1,318 | 10 (4.7), 226/236 | 7 / 110 / 81 | 3,109; 3,324; 1,233 |
| www.cnn.com | 8 (6.8), 155/428 | 11, 430/5,123 | 7 (5.9), 266/271 | 65 / 101 / 107 | — |
| www.foxnews.com | 36 (26.3), 147/155 | 68, 354/816 | 38 (27.8), 144/152 | 5 / 45 / 90 | 530; 460; 53; 199 |
| www.walmart.com | 43 (12.3), 167/205 | 2, 188 | 104 (29.8), 19/197 | 4 / 10 / 7 (11-04 visit: 13 / 180 layout) | 818; 5,056; 2,632; 402 |
| www.yahoo.com | 54 (32.6), 98/205 | 168, 136/1,380 | 65 (39.2), 57/59 | 16 / 27 / 14 | 4,265; 3,897; 2,464 |
| www.apple.com | 19 (16.7), 54/144 | 19, 160/976 | 24 (21.1), 47/57 | 6 / 28 / 12 | 3,292; 65; 77 |
| www.google.com | 3 (1.6), 52/56 | 25, 106/142 | 121 (63.0), 33/38 | 7 / 10 / 19 | 2,808 |

Timers: `relayout cause=timer` per second of dwell is the timer pressure the log can show
(yahoo 1.59/s, foxnews 0.76/s, wikipedia 0.23–0.50/s). There is no per-callback timer line;
the `Page lifecycle step done source="timers" callbacks=N` lines only cover the load
(amazon 1,994 callbacks, apple 188, wikipedia 60).

### Load phase, the 19-site list (headless, batch 25, 4 loads per site)

| site | load hold p50/max ms | relayouts/load | forced layouts/load | `Page scripts ran` p50 ms |
|---|---|---|---|---|
| www.microsoft.com | 16,465/17,098 | 6 | **92** | 7,346 |
| www.squarespace.com | 17,886/19,049 | 12 | 14 | 738 |
| github.com | 16,838/17,014 | 7 | 6 | 4,603 |
| www.lyft.com | 11,976/13,466 | 4.5 | 2 | 1,384 |
| x.com | 10,870/11,852 | 3 | 3 | 2 |
| www.netflix.com | 10,695/11,143 | 11 | 18 | 1,010 |
| www.facebook.com | 10,241/16,227 | 3 | 0 | 4 |
| www.instagram.com | 9,127/9,718 | 4 | 0 | 1,194 |
| www.bing.com | 8,926/9,465 | 4 | 0 | 56 |
| www.apple.com | 8,809/9,413 | 13 | 23 | 1,014 |
| www.walmart.com | 8,173/11,421 | 3 | 1 | 1,589 |
| www.youtube.com | 6,551/7,721 | 3 | 2 | 165 |
| www.yahoo.com | 6,139/8,165 | 4 | 0 | 351 |
| www.linkedin.com | 4,468/8,306 | 7 | 9 | 292 |
| en.wikipedia.org | 4,345/4,411 | 13 | 2 | 17 |
| weather.com | 4,066/4,717 | 5 | 0 | 444 |
| www.google.com | 2,497/2,650 | 3 | 3 | 893 |
| www.reddit.com | 222/257 (challenge page) | 2 | 0 | 0 |
| www.cnn.com | no hold line (30 s cap hit on all 4) | 3 | 0 | — |

Across the 72 load holds: 650 s total, of which script 175 s, build 24 s, layout 26 s,
snapshot 23 s; the rest is fetch waits (doc/sheets/fonts/images/script net buckets).
No budget stops and no image drops in any headless load.

## 2. Input-to-paint proxy

**As specified (pointer event that hit the budget → the next hold within 2 s):**
reddit 1,080 samples, 105/424/426 ms (p50/p95/max); walmart 43 samples, 142/144/144.
Every sample is a leaving-document stop (fix #892), so this proxy measures the stale hover
restyles the leaving page still ran: reddit holds after the stops were `move` 890, `leave`
166, `press` 14, `click` 4, `load_html` 6. With #892 on develop it should drop to zero samples.
It can't be the forward metric.

**Usable proxy for live pointer input:** the `move` hold (a pointer move that changed hover
state and relaid out) together with `Input to present`. On the flag-off logs move holds are
480 in total, 166.7 s, and p95 per site is in the table above: ebay 975, reddit 487,
en.wikipedia 221, walmart 205, yahoo 205, foxnews 155, apple 144, google 56 ms. Input to present
p95 per 100 inputs runs 53 ms (foxnews, best) up to 32 s (ebay, input queued behind 1 s hover
relayouts and 5 s live turns).

## 3. Mechanisms ranked by wasted ms (flag-off hand tests, 2,047 s of dwell)

Tonight's flag-on log puts a new item on top, animation relayouts (125 s; section 4, LAT2).
The table below is the flag-off ranking. Holds never nest, and relayouts never nest, so each family sums cleanly. The families overlap:
a hover relayout runs inside a move hold. Totals come from `--rank`.

| # | mechanism | total ms | n | where it goes | log evidence (shape) |
|---|---|---|---|---|---|
| 1 | **Hover/focus restyle takes the full relayout path** | 176,097 (move holds 166,748) | 737 relayouts | sheet-extract 74,367 / build 70,714 / layout 29,524 | `relayout cause=hover_or_focus … tree_reused=false took_ms=861.3 build_ms=201.5 layout_ms=42.2 … would_path=full full_reason=no_dom_record restyle_miss="a rule reads the pseudo-class beyond the element's subtree"`: 679 of 737 miss for that one reason (S22/#894's target, landed after these logs) |
| 2 | **Live turns: script plus a full relayout per turn** | 225,638 | 417 holds | script 80,713 / build 40,526 / layout 30,982 / snapshot 32,607 / other 39,428 | `Content thread hold entry=live_turn ms=4246 build_ms=715.2 layout_ms=558.2 … snapshot_ms=350.5 … script_ms=…` (ebay p50 974 ms; yahoo 168 holds) |
| 3 | **Load holds** (page thread held through the load) | 243,047 | 34 | script 69,672 / build 16,961 / layout 10,338 / snapshot 6,975 | `Content thread hold entry=load ms=19366 … script_ms=9010.9 script_turns=60 script_longest_ms=5305.3 … images_ms=2706.3` (ebay); reddit 13,117; cnn 26,136 (S23/S14 territory) |
| 4 | **Stylesheet + custom-property extraction redone on every full relayout** (cross-cuts 1, 2, 5) | 95,032 | every full relayout | ebay ~600 ms each (741 custom properties, 9 sheets), cnn 65, yahoo 12–16 | time from relayout start to `build_layout_tree: Extracted stylesheets and CSS variables inline_count=5 external_count=4 css_var_count=741`; `build_ms` does not include it, so the relayout line shows it only as `took_ms` minus the stages |
| 5 | **Timer-driven full relayouts** | 47,812 | 361 | extract 14,516 / build 18,697 / layout 13,553 | `relayout cause=timer causes={timer:1,script_mutation:1640} … would_path=full full_reason=has` path reasons: `full/anywhere` 142, `full/has` 123, `subtree` 82 |
| 6 | **Long script turns cut by the watchdog** (on time; the budget is what's left of the cumulative one) | 42,168 | 8 | ebay 4 (4.8–6.3 s), cnn 2 (15.0 s and 55 ms), yahoo 1 (1.1 s), ebay 11-04 1 (5.0 s) | `Watchdog: a script turn ran past its budget; interrupting it turn=259 ran_ms=5005 budget_ms=5000` + `Script stopped at the script budget source=timers elapsed_ms=5269` |
| 7 | **Snapshot stage inside holds** | ~55,800 | — | live_turn 32,607, move 9,448, load 6,975, scroll 6,034 | en.wikipedia live_turn snapshot p50 436 ms; scroll holds are snapshot-bound (`entry=scroll ms=94 build_ms=0.0 layout_ms=0.0 … snapshot_ms=87.8`) |
| 8 | **Input dispatched to a leaving document** | 11,174 of holds + 1,303 refusals | — | move 9,608, leave 1,566 | `Navigating content to: …` then 50–100 `Script stopped … source=event:pointerover elapsed_ms=0` and `entry=leave ms=416 build_ms=388.5` before the next `Loading URL` (fixed by #876/#892) |
| 9 | **Forced layouts in script tasks** | not logged (debug level); 4,600–13,100 at S16's 13–37 ms each | 354 (+703 in 76 headless loads; microsoft 92/load) | — | `Forced layouts in the script task id=… forced_layouts=5`; the per-layout `Forced layout ms=` line is `debug!` |
| 10 | **Images dropped at the 8 s subresource budget** | not a hold; visual loss | 145 (ebay 143, reddit 2) | ebay drops 69 in one millisecond | `Image over the subresource budget; rendering without it url=…` x69 at the same timestamp |

## 4. Is the churn the chrome's?

**Flag off (all three earlier hand-test logs): no.** None of them has the
`HIWAVE_RUSTKIT_CHROME=1` line. The chrome is the WebKit view in its own process and posts
only `Chrome IPC` lines. Every relayout, hold and stop in sections 1–3 is EngineViewId(2), the
content view. In those logs the churn is all engine.

**Flag on (tonight's `live-20261105T2246.log`, plus the real-window flag-on runs): yes, there is
a chrome share, and it's a specific one.** With the flag on, the chrome is a RustKit view.
"RustKit view created on the content thread" is logged for each view, but each gets its *own*
content thread (below).

**Chrome view, flag on (tonight, 331 s):** the chrome view did **7,469 relayouts, 55.9 s**
in total. 6,980 of them are `relayout cause=animation … would_path=full full_reason=no_dom_record`
at 6.7 ms p50 (52.5 s), and every one of those took the full path. The rate follows page
loading: 66 animation relayouts in the 10 s after the first load settled, then **~40 per second
(390–440 per 10 s)** from the walmart load at 03:48:37 to the end of the read, while ebay and
cnn loads kept a load or live turns pending. The chrome's infinite CSS animations are the likely
source: `.loading-indicator.active` (loadingPulse + loadingGlow, `infinite`), `.tab.loading`
(tabLoading, `infinite`) and decayPulse. Each is a transform/opacity animation that WebKit runs
on its compositor for free, and RustKit relays out for it every frame. The Inspector view
(`HIWAVE_RUSTKIT_CHROME=1: the Inspector is a RustKit view`) added 274 relayouts (1.9 s) and 100
forced layouts.

**But it runs on its own thread, not the page's.** Each RustKit view spawns its own
`ContentThread` (`webview_rustkit.rs`, `ContentThread::spawn` per view). The log agrees: 1,963
chrome relayouts finished *inside* the 391 page relayouts longer than 50 ms, which can't happen
on one thread. So the chrome animation costs about 17% of a core (55.9 s / 331 s) plus a
present per frame through the window-thread presenter. It does **not** show up in the page's
`Content thread hold` lines, and it doesn't explain page holds.

**Page side, same flag-on log (develop with #892, #894, S13):**

| site | dwell s | live stops/min | holds n, p50/p95 | biggest relayout cost | Input to present p95 per 100 inputs |
|---|---|---|---|---|---|
| www.cnn.com | 106 | 1.13 | 223, 359/473 | **`animation` 199 x 354 ms = 71.0 s** (build 123 / layout 189, all `full/no_dom_record`); live_turn holds 204 at p50 359 | — |
| en.wikipedia.org | 46 | 0 | 21, 1,582/4,659 | `timer` 16 x 659 ms (layout 446) | 39; 40 |
| www.amazon.com | 50 | **6.02** | 101, 113/3,544 | `timer` 40 x 63, `animation` 29 x 63, 86 forced layouts | 141; 95; 40; 40; 219 |
| www.walmart.com | 33 | 1.81 | 71, 251/513 | `timer` 55 x 226 (`full/has` 54), hover 13 x 230 (layout 208) | 1,053; 26; 5,339; 39; 31; 39 |
| www.ebay.com | 47 | 1.27 | 67, 223/341 | hover 59 x 210 ms (was 853 flag-off pre-#894), now `restyle_miss: a rule reads the pseudo-class where no compound shows it` | 26–43 on 8 lines; 6,525; 17,634; 29,988 during loads |

Mechanism totals, flag on (all views): relayouts `animation` 125.3 s (chrome 52.5, cnn 71.0,
amazon 1.7), `timer` 28.2 s, `hover_or_focus` 18.8 s; holds `live_turn` 168.2 s, `load` 99.4 s,
`move` 16.4 s.

**Answer to Pete:** the churn in the flag-off logs is all engine. With the flag on there is a
real chrome churn: ~40 full relayouts per second of the chrome document while a page is
loading, from its infinite CSS animations. It's on the chrome's own thread, so it costs CPU and
presents rather than page-thread time. The biggest churn in tonight's log is the **page's**
CSS animations on cnn: 71 s of full relayouts in 106 s. They started ticking with S13, and
nothing makes them paint-only yet (S18).

### Tonight's flag-on log, question by question (ranked by page-thread ms wasted)

View ids from the spans: chrome = EngineViewId(3) (`Loading HTML content` at start, `#urlInput`,
`#tabs`), content = EngineViewId(4) (every `Loading URL`), Inspector = EngineViewId(6). Of the
7,230 `relayout cause=animation` lines, 6,980 are the chrome's, 229 the content's and 21 the
Inspector's. Script-budget stops: 9, all live (timers 4, network 2, page script 1, cnn inline#12
15,251 ms, DOMContentLoaded 528 ms); 0 on pointer events and 0 leaving.

1. **Load holds, 99.4 s (16 holds).** The 33,314 ms one is the ebay load: script 14,437 ms in 49
   turns (longest 5,731, watchdog-cut), script network wait 9,905, doc fetch 3,957, images
   1,677, sheets 1,268, layout 732. It's script and network, not layout. Next are cnn at 24,626
   (layout 7,536, build 3,872, images 3,569, snapshot 2,827) and walmart at 12,754 (script
   fetch 4,357, doc fetch 2,739).
2. **CSS animations take a FULL relayout per animation frame: 72.8 s of the page thread.** Yes,
   this is S13 without S18. All 229 content `animation` relayouts are `would_path=full
   full_reason=no_dom_record tree_reused=false`. cnn: 199 in 80.5 s at **354 ms each (build 123,
   layout 189)**, one every 377 ms p50. The thread is saturated, so the animation runs at ~2.7
   frames/s, and it's most of cnn's 204 live_turn holds (77.4 s). amazon's hero: 29 in 10 active
   seconds, up to 6/s (3/s median, 168 ms apart), **63 ms each (build 23, layout 19, sheet
   extract 15)**. The chrome adds 6,980 more on its own thread (above).
3. **Timer-driven relayouts, 28.2 s.** walmart 55 at a **~280 ms cadence** (interval p10/p50/p90
   256/282/529 ms). 54 of the 55 are `causes={timer:1,script_mutation:1} mutations={child_list:1}`:
   one node inserted or removed per tick (a rotating/countdown element), yet each takes
   `would_path=full full_reason=has` (a `:has()` rule in the sheet forces the full path) at
   **226 ms (build 14, layout 206)**. en.wikipedia 16 at 659 ms (layout 446). amazon 40 at 63 ms.
4. **Hover relayouts, 16.3 s content** (ebay 12.3, walmart 2.9, amazon 1.1). ebay's 59: the short
   restyle path from S22 (#894; the plan's S22 row lists #894) is tried and misses on every one with
   `restyle_miss="a rule reads the pseudo-class where no compound shows it"`, so each falls to
   `full/no_dom_record` at **210 ms (build 44, layout 111)**. That's down from 853 ms pre-#894
   (a different miss reason then), but not the restyle-only path. walmart's 13 miss the same way
   at 230 ms, layout-bound.
5. **Long script turns cut by the watchdog: 45.5 s** (9 cuts; amazon ~5 s x 4, cnn 15.0 s).

**Live-turn holds: 351, p50 356 ms, p95 1,582, max 5,380.** Inside them (sums): layout 74.5 s,
script 37.3 s, build 33.9 s, other 11.8 s, snapshot 7.4 s, display list 2.5 s. Per hold p50:
layout 189, build 121, other 39, display list 3.9, script 0.1. **Layout dominates and script
barely runs**: the typical live turn is a timer or animation tick that changes almost nothing
and pays for a full build plus layout. By site: cnn 204 (77.4 s), amazon 71 (39.3 s), walmart 55
(20.9 s), en.wikipedia 17 (28.6 s), chrono24 4 (1.9 s).

**Engine, not chrome:** every hold above, and every relayout in items 1–4, is EngineViewId(4).
The chrome view's relayouts run on its own thread, and none of them is in these holds.

Real-window checks on local pages agree. 30 flag-on runs (batch 24/25 `rw-on1`): the chrome view
does 44–67 relayouts in its first 1.2 s (boot plus about 25 `animation` frames). After that it
runs 2–15 relayouts/s at 4–6 ms each (dropdown, context menu and typed-URL runs: 99–150 per
run). No load there stays pending, so the loading animation stops.

Caveat: tonight's build is develop after #892/#894/S13, and the flag-off logs are older builds,
so page-side numbers between the two aren't a clean A/B. The chrome-view relayouts are,
because they carry their own view id. A clean A/B of the chrome's share needs the same build
run flag off and flag on over the same sites. Hold lines carry no view id, so with the flag on
the tool gives every hold to the content view. Chrome relayouts are under the 50 ms hold
threshold, so this changes nothing today.

## 5. Proposed rows

Tooling first (Pete 10-22): LAT1 makes the metric and mechanisms 4 and 9 measurable. After
that, order follows the ranking, with tonight's flag-on log weighing in (LAT2 is its biggest item). Every red is a headless test on a reduced page the
lane writes itself.

| Row | What | Red (headless, reduced page) | Expected effect |
|---|---|---|---|
| **LAT1** (tooling) | Per-visit hold summary: `Content thread holds n=… p50_ms p95_ms max_ms by_entry={…}` over **all** entries, logged at navigation and every 100 entries. Also log `extract_ms` on the relayout line and promote the forced-layout total to the `Forced layouts in the script task … ms=` line. | A page with 10 cheap clicks and 1 slow one logs `n=11` with p50 under 50 ms; a relayout line on a 700-custom-property page has `extract_ms>0`; a forced layout's ms appears at info | Makes the exit metric computable from logs; makes mechanisms 4 and 9 visible. No latency change |
| **LAT2** (animations, page and chrome) | **S18, promoted:** transform/opacity/colour animations update the display list without a relayout. Tonight's log: cnn 199 animation relayouts x 354 ms = 71 s of the page thread in 106 s, and the RustKit chrome 6,980 x 6.7 ms ≈ 40/s on its own thread from `.loading-indicator.active`/`.tab.loading`/decayPulse. Until it lands, the chrome can pause its infinite animations after N cycles. | S18's red: a 500-item page with one `@keyframes` transform spinner shows 0 relayouts per frame after the first; plus the chrome HTML with `.loading-indicator.active` shows 0 animation relayouts over 1 s | cnn live_turn holds p50 359 → under 50 ms; chrome relayouts ~40/s → ~0. **Should land before CH3.A5 (flag default on)** |
| **LAT3** | Hover restyle without a full relayout, part 2. #894 measured on tonight's log: ebay hover 853 → 210 ms p50, but all 59 now miss with `restyle_miss: a rule reads the pseudo-class where no compound shows it`; walmart 230 ms, layout-bound (layout 208). Next: (a) the new miss reason on ebay's sheet shape; (b) dirty-subtree relayout for layout-changing hovers (walmart); (c) reuse the previous move's boxes; (d) `:has(:hover)`. | 1,000-row page with `li:hover{padding-left:2px}`: a move relays out <5% of boxes and layout_ms < 5; a reduced ebay-shape rule (pseudo-class in a compound that isn't the subject's) takes the short path | move holds: ebay p50 222 → <50, walmart 241 → <30. Flag-off move-hold total was 166.7 s |
| **LAT4** | Cache extracted stylesheets and custom properties across relayouts while no sheet, `<style>` or `style` attribute changed (sheet generation counter). | Page with 9 sheets and 741 custom properties: second hover relayout's extract_ms (LAT1) < 5% of the first | flag-off: ebay ~600 ms of every full relayout, 95 s total. Tonight it's lower (ebay 51, cnn 45 ms per relayout), but it still rides every animation/timer frame: 13.0 s of the 125 s of animation relayouts |
| **LAT5** | Pointer-event budget policy (**S16, Pete's decision**, below). | See options | Live input stays responsive while a page script is heavy |
| **LAT6** | Timer relayouts take the full path for `:has`/anywhere invalidation (`full/anywhere` 142, `full/has` 123). Scope `:has()` invalidation to the subjects whose argument matched, and batch timer mutations to one relayout per frame. | Page with `.card:has(.x)` and a 100 ms timer toggling `.x` on one of 500 cards: relayout path `subtree`, under 10% of boxes | tonight: walmart 55 timer relayouts x 226 ms (`full/has` 54), en.wikipedia 16 x 659; flag-off: yahoo 158 timer relayouts at 38 ms, foxnews 62 at 145, en.wikipedia 30 at 226: about 48 s of the flag-off total |
| **LAT7** | Forced layout from a clean tree: `getBoundingClientRect`/`offsetWidth` after no write returns the cached geometry, and reads after a write lay out only the dirty subtree. | Script that reads `offsetWidth` 100 times in a loop with no write: 0 forced layouts; one write then a read: one subtree layout | microsoft 92 forced layouts/load. With LAT1's ms, sized against S16's 13–37 ms each (≈1.7 s of microsoft's 5 s budget) |
| **LAT8** | Snapshot stage: en.wikipedia live_turn snapshot p50 436 ms; scroll holds are 90% snapshot. Find what the snapshot copies per frame and make it incremental. | 5,000-node page scrolled by 10 px: snapshot_ms < 5 | scroll holds (12.1 s) and part of live_turn (32.6 s snapshot) |
| **LAT9** | Image subresource budget: ebay drops 69 images in one millisecond at the 8 s budget, during a live turn. Make the budget per image from its own request start, not per batch, and let a late image land in a later frame instead of being dropped for the page's life. | Local server delays 20 images by 9 s: all 20 paint after arrival, and no `Image over the subresource budget` line ends with an image missing | ebay 143 drops → 0 (visual: H24 "eBay icons") |
| **LAT10** | Long script turns after load: the watchdog cuts on time (`ran_ms` = `budget_ms` + about 5 ms on all 8 cuts), but the budget a turn gets is what's left of the cumulative one: cnn's inline turn got 14,960 ms, ebay's timer turns 4,760–6,295. Option for Pete: cap any single post-load turn at 1 s (a timer or handler that runs longer is stopped and logged), separately from the load's cumulative budget. JS speed itself stays S14. | After `load`, `setTimeout(()=>{const t=Date.now();while(Date.now()-t<8000){}})` is stopped within 1,000 + 50 ms, and a 900 ms turn still completes | ebay live_turn p95 5,397 → ≤ ~1,200 ms; cnn 15 s hold → 1 s. Risk: a page whose real work needs >1 s loses it (Chrome doesn't stop it, it shows "page unresponsive") |

### S16 / LAT5: pointer-event budget options, with numbers

Today a pointer handler gets the live-turn budget (5 s), and the watchdog is the only limit.
In the flag-off logs **no live pointer handler was ever stopped**. The pointer cost is the
**restyle after the handler** (mechanism 1, a 19–853 ms p50 hover relayout per move) and moves
queued behind it (ebay `Input to present` p95 11–32 s). Options:

| Option | What it does | Effect on these logs | Risk |
|---|---|---|---|
| A. Shorter budget for move-class events (e.g. 50 ms for move/over/out/enter/leave) | Cuts a slow move handler | Nothing measured: the handlers here took ~0 ms; the time is in the restyle | Breaks drag/hover menus whose handlers do real work |
| B. Coalesce moves per frame (one `pointermove` per frame, latest position; enter/leave/over/out still delivered in order) | Bounds dispatch + restyle to 1 per frame | ebay: 200 move holds → at most 1 per 16 ms. When a restyle takes 850 ms, though, a frame is 850 ms, so it bounds the **queue** (Input to present 32 s → ~1 s) but not the per-move cost | Low; Chrome does this (#876 already coalesces adjacent queued moves) |
| C. Run move handlers off the critical path (dispatch after the frame's paint) | The paint for the previous input isn't held by move handlers | Small here (handler time ~0); helps sites with heavy move handlers | Ordering subtleties with click |
| D. Cancel on next move (drop a queued move's restyle when a newer move arrives before it starts) | Discards stale work | Same as B for the queue; with B it removes the 11 s backlog shape | Low |

Recommendation for Pete: **B + D now** (bounded queue, no behaviour risk). Keep the 5 s budget
for handlers (A buys nothing on this data). The latency itself is LAT3/LAT4 (the restyle). The
original S16 question (should forced-layout time count against the script budget?) waits on
LAT1's forced-layout ms. Microsoft's 92 forced layouts per load are the case to size it on.

## 6. How to re-measure

```
python3 tools/latency/baseline_from_logs.py ~/Library/Logs/hiwave/live-<new>.log            # per-site table + detail
python3 tools/latency/baseline_from_logs.py --rank ~/Library/Logs/hiwave/live-<new>.log     # section 3 totals
python3 tools/latency/baseline_from_logs.py --views ~/Library/Logs/hiwave/live-<new>.log    # chrome vs content (flag on)
python3 tools/latency/baseline_from_logs.py <capture-dir>/*.stderr.txt                      # headless A/B load phase
```
