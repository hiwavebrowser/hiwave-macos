# Chrome ground truth: what pinned Chrome does to load the board sites (2026-09-26)

Pete's question: can we learn from how Chrome itself loads these pages, and which libraries they use?
This probes pinned Chrome for Testing 148.0.7778.216 on the 20 board sites plus toyota, bmw, squarespace
and carvana (from the 80-site list), at 1280x800 with the oracle's deterministic launch flags.

**Tool:** `trench/tools/chrome_groundtruth.mjs` makes three loads per site, in one fresh browser per site with a 90 s budget:
1. **Coverage pass:** JS coverage at function granularity, CSS rule-usage coverage (both taken at first contentful paint (FCP) and again at load+5 s), a library census, and counts of Web API calls wrapped by an init script.
2. **Clean pass:** no instrumentation; gives load and FCP.
3. **Trace pass:** tracing only; gives main-thread self time per phase.

The passes are separated because CSS rule-usage tracking and precise coverage inflate style and V8 time by large factors. In the first combined run, github showed 12 s of "style"; in the trace-only run it's 110 ms. Tables come from `trench/tools/analysis/groundtruth_report.py`. The raw traces are in the session scratchpad and are not committed.

**Caveats:**
- Headless Chrome with SwiftShader, on a Mac shared with other seats, one sample per site. Treat numbers as ±30%.
- `other` is mostly RunTask self time: scheduler work and uninstrumented Blink.
- Blocked from this Mac: amazon, reddit, x, chatgpt, ebay, nytimes. carvana is also blocked: headless Chrome got Cloudflare's "Just a moment…" page. RustKit got carvana's real page, so its oracle is blocked, not RustKit.
- yahoo's trace pass timed out once at 90 s and passed on a retry with a 120 s budget.
- RustKit's phase numbers come from the trench's develop board run `20260926T1915Z-dev` (log-gap timeline, `trench/tools/analysis/timeline.py`), from before #286 and #287.

## 1. Top findings

1. **In Chrome, JavaScript dominates the main thread.** Of attributed main-thread time across 17 scorable sites, 69% is script, 13% paint, 7% style, 5% layout and 3% parse. The median site spends 448 ms in JS against 67 ms in style and 37 ms in layout. Main-thread busy time has a median of 1.25 s, clean FCP a median of 0.95 s, and clean load a median of 2.3 s.
2. **RustKit's style cascade is 14–69× slower than Chrome's, and its layout 9–100× slower, on the sites where RustKit parses the full CSS.** Style: cnn 9.7 s vs 210 ms, github 6.4 s vs 110 ms, wikipedia 1.4 s vs 20 ms. Layout: wikipedia 1.9 s vs 18 ms. Chrome runs hundreds of small incremental recalcs (cnn: 375 recalcs over 23k elements in total). RustKit does 2–3 full cascades of the whole tree. The comparison is only apples-to-apples where RustKit keeps the whole stylesheet; on yahoo and weather it still drops or skips much of it, so their low ratios are not wins.
3. **About half of the JS never runs.** Pooled over 139 MB shipped: **19% of bytes executed by FCP and 51% by load+5 s**. Per site, the median is 23.5% by FCP and 54% by load+5 s (range 32–67%). CSS is sparser still: a median of **12.9% of CSS bytes is used** by load+5 s (bmw 2.6%, youtube 2.4%).
4. **RustKit barely runs JS today.** github 0 scripts, microsoft 0, facebook 1. When it does run them, Chrome's JS cost becomes RustKit's biggest problem. Chrome spends 3.0 s in JS on microsoft, 2.6 s on weather and 1.6 s on yahoo, all with V8's JITs. Boa is an interpreter. **The Boa-vs-V8 ratio is not measured here and should be next.** If it is 10× or more, running all load-time JS in Boa is not viable on the heavy sites, whatever else we fix.
5. **The Web platform APIs the JS track needs are very consistent across sites.** On nearly every scorable site: Promise (17 of 17 sites), WeakMap/WeakSet (16), getComputedStyle (16, 72 calls at the median), getBoundingClientRect (16), matchMedia (16), IntersectionObserver (16), requestAnimationFrame (15), XHR (15), fetch (14), MutationObserver (14). Two of those, getComputedStyle and getBoundingClientRect, are **layout-synchronous**: JS reads laid-out geometry and style in the middle of a script. RustKit's DOM bindings will need forced synchronous style and layout, which is an architecture requirement, not a binding detail.
6. **Frameworks:** React is on 9 of 17 sites (facebook, instagram, yahoo, microsoft, netflix, github, cnn, weather, squarespace; Next.js on yahoo and weather). core-js polyfills are on 9. Tag managers: GTM 5, gtag 5, Adobe Launch 3. Lit is on 2 (microsoft, github), Vue on 1 (toyota), jQuery on 3 (wikipedia 3.7.1, microsoft 3.5.1, bmw 3.6.0). **No Angular, Svelte or Nuxt** on this set. Custom elements matter on two big sites: youtube defines 72 (Polymer with the ShadyDOM polyfill, 0 open shadow roots), and microsoft defines 55 with **248 open shadow roots**, which is its blank-header cause.

## 2. Chrome vs RustKit, per phase

| site | Chrome style ms | RustKit cascade s | ratio | Chrome layout ms | RustKit layout s | ratio | Chrome JS ms | RustKit scripts run / script s | Chrome main-thread busy ms | RustKit total s |
|---|---|---|---|---|---|---|---|---|---|---|
| cnn | 209.7 | 9.7 | 46x | 81.3 | 1.4 | 18x | 447.5 | 11 / 2.7 | 1246.2 | 23.9 |
| microsoft | 107.4 | 2.5 | 23x | 54.4 | 0.5 | 9x | 2969.7 | 0 / 0.0 | 3886.6 | 13.7 |
| github | 109.6 | 6.4 | 58x | 45.3 | 1.8 | 40x | 325.8 | 0 / 0.0 | 1316.2 | 13.1 |
| weather | 88.8 | 0.1 | (1x) | 102.5 | 1.2 | 11x | 2592.7 | 137 / 2.2 | 4160.1 | 9.2 |
| instagram | 16.4 | 0.0 | - | 14 | 0.1 | 4x | 216 | 3 / 5.1 | 395.4 | 9.1 |
| yahoo | 231 | 0.3 | (2x) | 338.2 | 0.4 | 1x | 1581.7 | 151 / 2.1 | 4225.1 | 7.8 |
| google | 5.6 | 0.0 | - | 9.2 | 0.2 | 17x | 195 | 14 / 2.4 | 287.2 | 7.4 |
| apple | 68.7 | 1.0 | 14x | 34.3 | 0.8 | 22x | 75.7 | 9 / 0.7 | 354.9 | 7.0 |
| netflix | 46.1 | 0.1 | 3x | 32.8 | 0.8 | 24x | 373.4 | 10 / 2.8 | 748.5 | 6.8 |
| wikipedia | 20.3 | 1.4 | 69x | 18.3 | 1.9 | 102x | 62.2 | 4 / 0.1 | 172 | 5.4 |
| facebook | 13.3 | 0.6 | 42x | 17.7 | 0.5 | 29x | 138.5 | 1 / 0.0 | 370.3 | 4.6 |
| youtube | 21.6 | 0.1 | 5x | 12.9 | 0.1 | 5x | 641.3 | 41 / 0.3 | 844.6 | 4.6 |
| linkedin | 19.9 | 0.1 | 7x | 37.1 | 0.5 | 13x | 308 | 5 / 1.2 | 450.6 | 2.8 |
| bing | 62.9 | 0.0 | - | 44.1 | 0.4 | 9x | 689.2 | 11 / 0.2 | 1310.6 | 2.4 |

Ratios in parentheses are sites where RustKit still drops stylesheets or runs little, so less work is being compared.
RustKit "script s" is RustKit's own script phase; Chrome's JS column is V8 self time over far more code.

**Lessons for the cascade** (from Blink's behaviour, BSD, described here not copied):
- Chrome's style work per element is small because it avoids most matching and most recomputation. Several mechanisms do this:
  - Rule-set buckets by id/class/tag/attribute. RustKit has these since #256/#257.
  - An ancestor Bloom filter, which rejects descendant selectors without walking ancestors. RustKit has no equivalent.
  - Style sharing between siblings with the same matched rules.
  - A matched-properties cache, which reuses computed values when the same rule set applies.
  - Incremental invalidation, which recomputes only elements whose inputs changed. RustKit re-cascades the whole tree on every relayout.
- The two cheapest wins: a Bloom filter over the ancestor chain in `rule_may_match`, and a matched-properties cache keyed on the matched-rule list plus parent style.
- The structural one: incremental invalidation instead of a full re-cascade per relayout (2–3 per load today).

## 3. Chrome ground-truth tables

sites: 24, scorable (not blocked/challenge): 17; blocked: amazon, reddit, x, chatgpt, ebay, nytimes, carvana

### Phases (Chrome main thread, self ms, trace-only run) and clean load/FCP

| site | clean FCP | clean load | parse | script | style (recalcs/elements) | layout (count/dirty) | paint | gc (minor/major) | other | JS heap MB | DOM |
|---|---|---|---|---|---|---|---|---|---|---|---|
| google | 940 | 1771 | 8.8 | 195 | 5.6 (24/199) | 9.2 (10/189) | 8.4 | 14.4 (6/1) | 45.8 | 12 | 489 |
| youtube | 1216 | 3017 | 33.9 | 641.3 | 21.6 (61/1395) | 12.9 (30/1661) | 9.7 | 12.1 (12/1) | 113 | 42.6 | 7037 |
| facebook | 948 | 1432 | 20.1 | 138.5 | 13.3 (21/450) | 17.7 (9/873) | 9.5 | 10.1 (7/1) | 161 | 13.5 | 687 |
| instagram | 772 | 2309 | 17.5 | 216 | 16.4 (36/2032) | 14 (13/681) | 41.5 | 11.9 (4/2) | 78.2 | 16.8 | 1061 |
| wikipedia | 336 | 558 | 14 | 62.2 | 20.3 (66/5032) | 18.3 (60/4428) | 13.9 | 2.9 (4/0) | 40.4 | 9.2 | 9438 |
| linkedin | 688 | 666 | 18.8 | 308 | 19.9 (29/1291) | 37.1 (20/889) | 7.7 | 21.4 (12/1) | 37.6 | 4.9 | 2849 |
| yahoo | 1152 | 9989 | 47.7 | 1581.7 | 231 (1628/19845) | 338.2 (1537/61278) | 766.6 | 205 (53/10) | 1054.9 | 81 | 7769 |
| bing | 1380 | 1207 | 31 | 689.2 | 62.9 (345/5363) | 44.1 (270/6946) | 198.4 | 25.1 (12/2) | 260 | 60.2 | 22379 |
| microsoft | 1876 | 4230 | 115.1 | 2969.7 | 107.4 (767/14413) | 54.4 (245/12384) | 191.1 | 31.1 (22/2) | 417.8 | 50.1 | 31922 |
| apple | 1708 | 3203 | 12.5 | 75.7 | 68.7 (267/7831) | 34.3 (140/5787) | 118.8 | 5.2 (5/1) | 39.8 | 6 | 4236 |
| netflix | 816 | 6339 | 22.7 | 373.4 | 46.1 (1232/12869) | 32.8 (48/3440) | 165.3 | 11.9 (13/1) | 96.3 | 36.1 | 1945 |
| github | 1620 | 1650 | 38.5 | 325.8 | 109.6 (163/9448) | 45.3 (57/9758) | 54 | 7.7 (8/1) | 735.3 | 22.4 | 3404 |
| cnn | 608 | 2134 | 70.1 | 447.5 | 209.7 (375/23311) | 81.3 (309/15416) | 159.5 | 18.6 (8/2) | 259.4 | 37.1 | 15385 |
| weather | 1124 | 13402 | 38.4 | 2592.7 | 88.8 (446/6301) | 102.5 (276/12765) | 343 | 115.7 (32/3) | 879.1 | 146.3 | 6741 |
| toyota | 684 | 2758 | 42.5 | 1083.5 | 146.2 (763/12921) | 101.8 (718/26802) | 279 | 32.7 (15/3) | 308.8 | 28.7 | 15363 |
| bmw | 360 | 1921 | 55.5 | 820 | 68.1 (274/5711) | 26.2 (254/8490) | 73.2 | 26.3 (74/4) | 419 | 42.4 | 12983 |
| squarespace | 1884 | 2984 | 15.9 | 1049.6 | 66.6 (374/8702) | 50 (89/8209) | 111.1 | 15.9 (14/1) | 206.4 | 79.6 | 6532 |
| carvana (blocked) | 2147 | 2576 | 5.1 | 503.2 | 4 (35/179) | 9.9 (17/197) | 2.9 | 14.6 (17/2) | 38.6 | 17.7 | 89 |

Median over scorable sites (self ms): parse 31, script 447.5, style 66.6, layout 37.1, paint 111.1, gc 15.9, other 206.4
Share of attributed main-thread time across scorable sites: script 69%, style 7%, layout 5%, parse 3%, paint 13%, gc 3%

### Coverage

| site | scripts | JS KB | JS run @FCP | JS run @load+settle | CSS KB | CSS used @FCP | CSS used @end |
|---|---|---|---|---|---|---|---|
| google | 35 | 2093 | 3.7% | 46.9% | 125 | 7.4% | 9.8% |
| youtube | 366 | 15017 | 5% | 39.5% | 4179 | 1.5% | 2.4% |
| facebook | 86 | 4828 | 30.8% | 32% | 1516 | 5.9% | 5.9% |
| instagram | 78 | 7188 | 0.2% | 34.3% | 1191 | 2.4% | 4.7% |
| wikipedia | 54 | 1163 | 7.7% | 61% | 382 | 14.5% | 19.1% |
| linkedin | 21 | 1551 | 35% | 37.1% | 334 | 21.5% | 21.5% |
| yahoo | 279 | 11476 | 38.1% | 57.7% | 754 | 10.3% | 12.9% |
| bing | 159 | 9080 | 1% | 55% | 776 | 3.8% | 25.4% |
| microsoft | 274 | 9559 | 41.2% | 67.3% | 954 | 18.5% | 21.5% |
| apple | 35 | 1115 | 52.5% | 53.3% | 674 | 11.9% | 11.9% |
| netflix | 59 | 6451 | 46.1% | 60.6% | 987 | 11.1% | 12.4% |
| github | 127 | 6400 | 23.5% | 34.4% | 5913 | 21% | 21.1% |
| cnn | 94 | 15021 | 13.2% | 49.2% | 2983 | 21.6% | 22% |
| weather | 588 | 19814 | 10.4% | 54.7% | 542 | 56.4% | 65.1% |
| toyota | 152 | 6890 | 24.8% | 53.7% | 1649 | 9.7% | 12.5% |
| bmw | 255 | 10603 | 15.6% | 53.5% | 3065 | 1.9% | 2.6% |
| squarespace | 111 | 14170 | 24% | 55.2% | 166 | 42.9% | 47.7% |
| carvana (blocked) | 32 | 14701 | 10.4% | 99.4% | 21 | 11.6% | 11.6% |

Pooled over scorable sites: JS 139.1 MB shipped, 19% executed by FCP, 51% by load+settle; CSS 25.6 MB.
Per-site JS executed by load+settle: median 54%, range 32–67%.

### Library census (scorable sites)

| library | sites | which |
|---|---|---|
| react | 9 | facebook, instagram, yahoo, microsoft, netflix, github, cnn, weather, squarespace |
| core_js | 9 | youtube, yahoo, bing, microsoft, cnn, weather, toyota, bmw, squarespace |
| ads | 6 | youtube, yahoo, cnn, weather, bmw, squarespace |
| gtm | 5 | yahoo, microsoft, toyota, bmw, squarespace |
| gtag | 5 | linkedin, yahoo, microsoft, toyota, bmw |
| adobe_launch | 3 | microsoft, cnn, bmw |
| next | 2 | yahoo, weather |
| lit | 2 | microsoft, github |
| vue | 1 | toyota |
| wc_polyfill | 1 | youtube |
| ga | 1 | microsoft |
| optimizely | 1 | cnn |
| nuxt | 0 |  |
| angular | 0 |  |
| svelte | 0 |  |
| segment | 0 |  |
| jquery | 3 | wikipedia 3.7.1, microsoft 3.5.1, bmw 3.6.0 |

Custom elements (defined/tags) and open shadow roots: google 0/3, 0 shadow; youtube 72/79, 0 shadow; yahoo 0/1, 1 shadow; bing 2/2, 2 shadow; microsoft 55/56, 248 shadow; github 6/6, 1 shadow; weather 0/1, 1 shadow; bmw 0/1, 0 shadow

### Web APIs called during load (scorable sites)

| API | sites | total calls | median calls/site (where used) |
|---|---|---|---|
| Promise | 17 | 60802 | 976 |
| WeakMap | 16 | 2548 | 128 |
| getComputedStyle | 16 | 16984 | 72 |
| getBoundingClientRect | 16 | 6914 | 37 |
| WeakSet | 16 | 2620 | 12 |
| matchMedia | 16 | 1598 | 25 |
| IntersectionObserver | 16 | 471 | 12 |
| XMLHttpRequest | 15 | 166 | 8 |
| requestAnimationFrame | 15 | 3926 | 90 |
| fetch | 14 | 487 | 18 |
| MutationObserver | 14 | 616 | 5 |
| requestIdleCallback | 13 | 742 | 34 |
| ResizeObserver | 12 | 321 | 5 |
| Intl.DateTimeFormat | 12 | 650 | 3 |
| Proxy | 11 | 3861 | 17 |
| crypto.randomUUID | 11 | 172 | 9 |
| CSS.supports | 10 | 595 | 1 |
| customElements.define | 8 | 1498 | 70 |
| structuredClone | 7 | 4097 | 4 |
| FinalizationRegistry | 5 | 19 | 1 |
| WeakRef | 5 | 176 | 20 |
| attachShadow | 5 | 588 | 2 |
| Intl.Locale | 3 | 5968 | 2805 |
| Intl.NumberFormat | 2 | 64 | 32 |
| document.fonts | 1 | 4 | 4 |
| Intl.PluralRules | 1 | 1 | 1 |
| Intl.Segmenter | 1 | 1 | 1 |

### WeakMap / WeakRef / FinalizationRegistry during load

| site | WeakMap | WeakSet | WeakRef | FinalizationRegistry | Proxy | Promise |
|---|---|---|---|---|---|---|
| google | 11 | 1 | 0 | 0 | 1 | 201 |
| youtube | 75 | 4 | 0 | 1 | 0 | 3801 |
| facebook | 358 | 14 | 20 | 0 | 0 | 90 |
| instagram | 495 | 10 | 24 | 0 | 5 | 119 |
| wikipedia | 0 | 0 | 0 | 0 | 0 | 2 |
| linkedin | 2 | 10 | 0 | 0 | 0 | 46 |
| yahoo | 155 | 49 | 104 | 8 | 133 | 6377 |
| bing | 290 | 1485 | 0 | 0 | 2 | 595 |
| microsoft | 42 | 77 | 0 | 0 | 402 | 11641 |
| apple | 52 | 1 | 0 | 0 | 53 | 19 |
| netflix | 108 | 5 | 9 | 8 | 0 | 387 |
| github | 194 | 58 | 0 | 0 | 2 | 976 |
| cnn | 67 | 4 | 0 | 0 | 0 | 1226 |
| weather | 160 | 789 | 19 | 1 | 3128 | 30083 |
| toyota | 128 | 7 | 0 | 1 | 117 | 1175 |
| bmw | 128 | 24 | 0 | 0 | 1 | 2216 |
| squarespace | 283 | 82 | 0 | 0 | 17 | 1848 |

## 4. What this means for the roadmap

- **B5 (performance) should target the cascade more than layout.** On cnn, github and wikipedia the style gap is the larger one in absolute time: 9.7, 6.4 and 1.4 s against Chrome's 0.2, 0.1 and 0.02 s. The Bloom filter and matched-properties cache are the next items, then incremental restyle.
- **The JS track (B6/B7) needs a gating measurement before more investment:** Boa vs V8 on these sites' actual scripts. Chrome needs a median of only 23.5% of shipped JS by FCP, so a first-paint-first policy can rescue first-viewport correctness without running everything: execute parser-blocking and inline scripts, then defer async/defer/module and idle work past the capture. Pair it with the script watchdog (B6) so a heavy tail can't hang the page.
- **DOM bindings priority**, from API use by site count:
  - timers and Promise jobs, requestAnimationFrame and requestIdleCallback;
  - getComputedStyle and getBoundingClientRect, with forced layout;
  - matchMedia;
  - IntersectionObserver, MutationObserver, ResizeObserver;
  - fetch and XHR;
  - customElements.define plus attachShadow (for microsoft, youtube, github);
  - Intl.DateTimeFormat, crypto.randomUUID, structuredClone and CSS.supports.

  React is the one framework that matters, on 9 of 17. If React's reconciliation and commit path run correctly, most of the JS-rendered sites follow.
- **The boa_gc weak-phase bug (#287):** WeakMaps are everywhere, with a median of 128 constructions per site at load. facebook (358), instagram (495), squarespace (283), github (194) and weather (160) create them heavily, and toyota and bmw create 128 each. The bug needs a reference cycle reachable from a live WeakMap value while Boa is actually running the page's scripts. That's why it hit toyota, bmw, squarespace and tripadvisor, where RustKit runs dozens of scripts, and not facebook or instagram, where RustKit runs 1–3. **Expect it on every React/Lit site once RustKit runs more of their JS.** #287 is a prerequisite for the JS track, not a one-off fix.
- **CSS bytes:** a median of 12.9% is used. Parsing all of it is still required, since selectors are matched lazily per element. But it argues for a fast reject path: rules whose rightmost compound can never match the document should cost close to nothing. The cascade index already moves in this direction.

## 5. Surprises

- carvana's oracle is Cloudflare-challenged for headless Chrome (HTTP 403, "Just a moment…"), while RustKit gets the real page. The HeadlessChrome UA (decision A1) blocks more oracles than x.
- Even Chrome loads yahoo (10.0 s clean; 1,537 layouts, 61k dirty objects) and weather (13.4 s; 588 scripts, 19.8 MB JS, 30k Promises, 3,128 Proxies, a 146 MB JS heap) slowly. Neither page is light for anyone.
- facebook runs 31% of its JS before FCP; instagram, same company, runs 0.2%. Server-rendered HTML against client-rendered.
- github ships 5.9 MB of CSS and uses 21% of it; youtube uses 2.4% of 4.2 MB.
- Chrome's JS heap is small: a median of 36 MB, max 146 MB (weather). A RustKit process that climbs into GBs on any of these sites is a bug, not load. Today's earlier measurements put RustKit's peak RSS at 0.1–0.7 GB, against Chrome's whole process tree at 0.8–2.6 GB.
