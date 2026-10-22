# Board ledger

Trace's standing board ledger for Atlas / ZeuzGb. It records the real-site board score at `develop` tips so a later drop can be read as: develop sha, board numbers on that sha, pull requests merged since the previous tip.

On a drop of the /60 score or the /75 score, Trace checks Chrome oracle drift first. An engine capture that is byte-identical to the previous tip is oracle drift, and no merge is blamed. When the engine capture moved, Trace bisects the pull requests listed on that row.

A score is entered only when a board run measured it on that sha. Anything unmeasured stays **TBD**.

**Order:** oldest first. Append a new tip at the bottom. Do not insert above the seed.

**Denominators**

- **/60** is the original 20 sites (`websuite/realsite-top20.json`).
- **/75** is all 25 sites. [PR #568](https://github.com/hiwavebrowser/hiwave-macos/pull/568) (Atlas, open at this seed, not an ancestor of `50e77c83`) grows the list to 25. The original 20 stay comparable on /60, and the full list is scored on /75. Both denominators are recorded together starting with the 4:30 AM run (`a7564057`, **26/75**).
- After [PR #617](https://github.com/hiwavebrowser/hiwave-macos/pull/617) (2026-10-08) the list is 26 sites (simonwillison appended). The full score max becomes 78. The table keeps the /75 column. From `3363c677` on, that column holds the full-board score, and the entry notes that the max is 78.
- #568's body also records a one-load reading of the five new sites on `481db9cb` (3/15). That reading is not a board score and is not a row below.

Seeded 2026-10-06 ET. The 27 and 26 below are that night's record of the github drop. They are not restated in a commit message on these shas.

## Score table

| Tip | Date (ET) | /60 | /75 | Per-site | Localisation |
|---|---|---:|---:|---|---|
| [`24185512`](#24185512--2026-10-05-1233-et) | 2026-10-05 12:33 | 27 | — | — | Before-side of the github drop. |
| [`e3b833fc`](#e3b833fc--2026-10-06-0418-et) | 2026-10-06 04:18 | 26 | — | github | Oracle drift. Do not blame #550. |
| [`50e77c83`](#50e77c83--2026-10-06-2036-et) | 2026-10-06 20:36 | not measured (no board ran on this tip) | not measured (no board ran on this tip) | TBD | No board rerun on this tip. |
| [`481db9cb`](#481db9cb--2026-10-06-1300-et) | 2026-10-06 13:00 | 26 | — | — | Same as `e3b833fc`. No drop. /75 not measured (before #568). |
| [`a7564057`](#a7564057--2026-10-07-0430-et) | 2026-10-07 04:30 | 24 | 26 | github 1→0, wikipedia 2→1 | No engine regression PR. github timing race; wikipedia Chrome oracle drift. |
| [`a181da05`](#a181da05--2026-10-07-1300-et) | 2026-10-07 13:00 | 26 | 29 | github 0→1, wikipedia 1→2, autotrader 1→2 | Rise. github is #598. wikipedia and autotrader are Chrome oracle drift. |
| [`7eb35d97`](#7eb35d97--2026-10-08-0430-et) | 2026-10-08 04:30 | 26 | 29 | — | Same as `a181da05`. No per-site change. |
| [`3363c677`](#3363c677--2026-10-08-1300-et) | 2026-10-08 13:00 | 24 | 29 | google 3→1; simonwillison appears (2) | Drop 26→24. No engine regression PR. Do not blame #617 or #623. Max is 78. |
| [`65e1e2f4`](#65e1e2f4--2026-10-09-0430-et) | 2026-10-09 04:30 | 26 | 31 | google 1→2, wikipedia 2→3, simonwillison 2→3, autotrader 2→1 | Recover 24→26. Prior google drop recovered. No new blame. Max is 78. |
| [`cea4c0c6`](#cea4c0c6--2026-10-09-1336-et) | 2026-10-09 13:36 | 26 | 31 | google 2→3, wikipedia 3→2 | No drop. Max is 78. |
| [`16f97285`](#16f97285--2026-10-09-1729-et) | 2026-10-09 17:29 | 27 | 32 | wikipedia 2→3 | Rise. Max is 78. |
| [`81ef8b34`](#81ef8b34--2026-10-12-0430-et) | 2026-10-12 04:30 | 26 | 31 | google 3→2 | Drop 27→26. No engine PR. Max is 78. |
| [`696f9921`](#696f9921--2026-10-12-1300-et) | 2026-10-12 13:00 | 26 | 31 | google 2→3, wikipedia 3→2 | No drop. Max is 78. |
| [`bbd92964`](#bbd92964--2026-10-13-0458-et) | 2026-10-13 04:58 | 27 | 32 | walmart 1→2 | Rise 26→27. Possible Chrome settling, maybe #683. Max is 78. |
| _next_ | | TBD | TBD | | |

## Entries

### `24185512` — 2026-10-05 12:33 ET

`24185512` → **27/60** → no earlier tip is seeded, so no merge list sits under the 27.

- Full sha: `2418551260aca300b9f84940be26764f72ba1851`
- This tip is the merge of [#530](https://github.com/hiwavebrowser/hiwave-macos/pull/530) (`bindings(cloud W4-O): MutationObserver records and microtask delivery`).
- /75: not in use.
- Per-site notes: none on this side of the drop. The point lost on the next tip is github.

### `e3b833fc` — 2026-10-06 04:18 ET

`24185512` → `e3b833fc` → **27/60 → 26/60** (github) → #516, #531, #541, #542, #544, #545, #546, #547, #548, #549, #550.

- Full sha: `e3b833fc7e7e2c28ae9db95009c6a6b78a2ca247`
- This tip is the merge of [#550](https://github.com/hiwavebrowser/hiwave-macos/pull/550).
- /75: not in use.
- Per-site: github lost the point.

**Localisation (Trace, 2026-10-06 ET, high confidence).** Chrome oracle drift in the github.com homepage hero, bottom cloud/planet graphic. `rustkit.png` is byte-identical, so this is not an engine regression. Do not blame #550.

First-parent merges on `develop` after `24185512` through this tip (oldest first):

| Merge | When (ET) | PR | Subject |
|---|---|---:|---|
| `7a3cf2ce` | 2026-10-05 14:10 | #516 | feat(bindings): NodeFilter, TreeWalker, NodeIterator (YouTube webcomponents-sd.js no longer dies on NodeFilter) |
| `c429072e` | 2026-10-05 14:10 | #531 | bindings(cloud W4-P): postMessage, MessageChannel and requestIdleCallback |
| `bdb24516` | 2026-10-05 14:11 | #541 | feat(parity-capture): click action resolves bounding box and clicks through engine (Z2-I1) |
| `feb0667c` | 2026-10-05 14:11 | #542 | fix(engine): a disabled control gets no mousedown and no mouseup (Z I0) |
| `6d9cfe55` | 2026-10-05 17:32 | #544 | fix(engine): a page scrolls to the bottom of its lowest box (I0, H6: viewport-tall bodies did not scroll) |
| `088d6579` | 2026-10-05 18:16 | #545 | feat(bindings): implement Intl.Segmenter (ECMA-402) in web_intl.js |
| `07b08442` | 2026-10-05 18:17 | #546 | feat(bindings): implement document.defaultView returning window in web_document.js |
| `d2edbc6a` | 2026-10-05 23:37 | #547 | feat(bindings): document.implementation, CDATASection and ProcessingInstruction |
| `c77bffaa` | 2026-10-05 23:38 | #548 | fix(layout): a flexible row of an auto-height grid is as tall as its items |
| `2084cb00` | 2026-10-06 04:18 | #549 | fix(layout): inline children of a grid item share lines |
| `e3b833fc` | 2026-10-06 04:18 | #550 | fix(layout): a grid item that mixes text with blocks, images or controls is flowed as a block flows it |

### `50e77c83` — 2026-10-06 20:36 ET

`e3b833fc` → `50e77c83` → **/60 not measured (no board ran on this tip), /75 not measured (no board ran on this tip)** → #551, #552, #543, #528, #553, #555, #557, #556, #562, #564, #561, #559, #477.

- Full sha: `50e77c838e5dbf87070a54562d2924926256d1af`
- Develop tip as of the 2026-10-06 ET seed. This tip is the merge of [#477](https://github.com/hiwavebrowser/hiwave-macos/pull/477) (`test: cover image loader, h2 header strip, calc shorthand, and subresource budget survivors`).
- Earlier that evening, [#564](https://github.com/hiwavebrowser/hiwave-macos/pull/564), [#561](https://github.com/hiwavebrowser/hiwave-macos/pull/561), and [#559](https://github.com/hiwavebrowser/hiwave-macos/pull/559) landed. The Mac was free again after those. No board rerun has been entered for this tip, including none after the 25-site change.
- /60: **not measured (no board ran on this tip)**. /75: **not measured (no board ran on this tip)**. The 4:30 AM run on `a7564057` is the first one that records both, and only once #568's list is what the board runs.
- Per-site notes: **TBD**.
- Localisation: none. There is no measured delta to localise.

First-parent merges on `develop` after `e3b833fc` through this tip (oldest first):

| Merge | When (ET) | PR | Subject |
|---|---|---:|---|
| `9563fe5a` | 2026-10-06 04:51 | #551 | fix(css): parse place-items, place-self and place-content |
| `146ca8ad` | 2026-10-06 10:14 | #552 | fix(layout): the block axis of grid alignment (align-items, align-self, align-content) |
| `84e11ff3` | 2026-10-06 10:38 | #543 | test: pin L0 class membership, initEvent dispatch, and wrapper clones |
| `07c2c9bb` | 2026-10-06 10:39 | #528 | test: cover privacy-pin destinations and escaped :is() selectors |
| `99c3bcfe` | 2026-10-06 11:11 | #553 | fix(layout): a grid item whose only child is text is flowed as lines |
| `8e91bb3d` | 2026-10-06 11:11 | #555 | fix(scorer_v2): support run_dir root JSON paths for live runs |
| `481db9cb` | 2026-10-06 11:12 | #557 | fix(layout): a grid item is bounded by min-width, max-width and max-height |
| `442815e6` | 2026-10-06 15:28 | #556 | record(interactive): Windows rerun at 5744c7ce with script error rankings |
| `03718a7c` | 2026-10-06 15:56 | #562 | fix(layout): anchor a positioned grid item's abspos children to its final box (#560) |
| `dcd46e39` | 2026-10-06 20:06 | #564 | fix(viewhost): the content view records the wheel (H6); a live resize lays out once per turn (H9) |
| `c0667111` | 2026-10-06 20:07 | #561 | fix(bindings): inherit window from Window.prototype -> EventTarget.prototype |
| `27111418` | 2026-10-06 20:27 | #559 | fix(js): bump Boa from 0.20 to 0.22 |
| `50e77c83` | 2026-10-06 20:36 | #477 | test: cover image loader, h2 header strip, calc shorthand, and subresource budget survivors |

### `481db9cb` — 2026-10-06 13:00 ET

`e3b833fc` → `481db9cb` → **26/60 → 26/60** (scorable 25/57, no drop), **/75** not measured (before #568) → #551, #552, #543, #528, #553, #555, #557.

- Full sha: `481db9cb03192f526d00945fedd666a7a8f81ff4`
- This tip is the merge of [#557](https://github.com/hiwavebrowser/hiwave-macos/pull/557) (`fix(layout): a grid item is bounded by min-width, max-width and max-height`), merged 2026-10-06 11:12 ET. The sha is an ancestor of the `50e77c83` seed. It is appended under that seed.
- Full 20-site quiet board at 2026-10-06 17:00Z (13:00 ET). Run dir: `trench/realsite/runs/20261006T1700Z-quiet-dev481db9cb`.
- /60: **26** (scorable 25/57), same as `e3b833fc`, so no drop. /75: not measured (before #568).
- Per-site notes: none. The score did not move.
- Localisation: none. There is no measured delta.

First-parent merges on `develop` after `e3b833fc` through this tip (oldest first):

| Merge | When (ET) | PR | Subject |
|---|---|---:|---|
| `9563fe5a` | 2026-10-06 04:51 | #551 | fix(css): parse place-items, place-self and place-content |
| `146ca8ad` | 2026-10-06 10:14 | #552 | fix(layout): the block axis of grid alignment (align-items, align-self, align-content) |
| `84e11ff3` | 2026-10-06 10:38 | #543 | test: pin L0 class membership, initEvent dispatch, and wrapper clones |
| `07c2c9bb` | 2026-10-06 10:39 | #528 | test: cover privacy-pin destinations and escaped :is() selectors |
| `99c3bcfe` | 2026-10-06 11:11 | #553 | fix(layout): a grid item whose only child is text is flowed as lines |
| `8e91bb3d` | 2026-10-06 11:11 | #555 | fix(scorer_v2): support run_dir root JSON paths for live runs |
| `481db9cb` | 2026-10-06 11:12 | #557 | fix(layout): a grid item is bounded by min-width, max-width and max-height |

### `a7564057` — 2026-10-07 04:30 ET

`481db9cb` → `a7564057` → **26/60 → 24/60**, **/75** 26 (first 25-site board after #568) → #556, #562, #564, #561, #559, #477, #565, #573, #576, #577, #578, #568, #581, #566, #572, #579, #582, #567, #580, #571, #588.

- Full sha: `a75640577e096e66b18446b8392b37c0a10a2a7c`
- This tip is the merge of [#588](https://github.com/hiwavebrowser/hiwave-macos/pull/588) (`docs(census): walls-by-vendor table for technique census top80 (follow-up to #572)`), merged 2026-10-07 00:35 ET.
- Board 2026-10-07 04:30 ET. Run dir: `trench/realsite/runs/20261007T0831Z-quiet-deva7564057`.
- /60: **24** (drop 26 → 24). /75: **26**. This is the first 25-site board after #568.
- Per-site: github 1 → 0, wikipedia 2 → 1.
- Localisation: no engine regression PR.

**github 1 → 0** is a timing race. GitHub's landing-pages bundle sometimes finishes inside parity-capture's 5000 ms script budget. React hydration then throws on a missing Boa API, and the ErrorBoundary replaces the page with `Looks like something went wrong!`. Per-sha script_stats (Pollux): `481db9cb`, `908634c8`, and `a456ccac` over_budget = PASS; `f95eb9c8` (#580 merge) threw = PASS; `a7564057` ran 3.1 s = FAIL. `a456ccac`..`a7564057` is docs-only. #580 was first named by Trace, then retracted. Atlas ruled no revert.

**wikipedia 2 → 1** is Chrome oracle drift. rustkit is identical (190 words, non-background 0.0997 vs 0.0999). Chrome oracle words 178 → 152 because the chrome-a capture at `a7564057` had Wikipedia's `Wiki Loves Monuments` CentralNotice banner (chrome-a and chrome-b disagree within each run, 335 vs 271 words), so the readable ratio 0.8146 → 0.7434 fell under the 0.80 line.

#573 is the 60 s script budget in the live app, not parity-capture. #566 is object-fit. #567 is svg `<use>`. #568 is the 25-site list. #571 is the warnings cleanup. #580 is script-started navigation. Docs in this window: #565, #572, #576, #577, #578, #579, #581, #582, #588. #556, #562, #564, #561, #559, and #477 also sit on the unmeasured `50e77c83` row, between these two boards.

First-parent merges on `develop` after `481db9cb` through this tip (oldest first):

| Merge | When (ET) | PR | Subject |
|---|---|---:|---|
| `442815e6` | 2026-10-06 15:28 | #556 | record(interactive): Windows rerun at 5744c7ce with script error rankings |
| `03718a7c` | 2026-10-06 15:56 | #562 | fix(layout): anchor a positioned grid item's abspos children to its final box (#560) |
| `dcd46e39` | 2026-10-06 20:06 | #564 | fix(viewhost): the content view records the wheel (H6); a live resize lays out once per turn (H9) |
| `c0667111` | 2026-10-06 20:07 | #561 | fix(bindings): inherit window from Window.prototype -> EventTarget.prototype |
| `27111418` | 2026-10-06 20:27 | #559 | fix(js): bump Boa from 0.20 to 0.22 |
| `50e77c83` | 2026-10-06 20:36 | #477 | test: cover image loader, h2 header strip, calc shorthand, and subresource budget survivors |
| `3c1f48ee` | 2026-10-06 21:38 | #565 | docs(diagnostics): Pete hand-test diagnostics (eBay images, top-20 icons, menus behind images) |
| `5f6a36f2` | 2026-10-06 22:31 | #573 | feat(app): 60 s script budget for pages in the live app (Z I0, approved) |
| `94742deb` | 2026-10-06 22:32 | #576 | docs(diagnostics): reduced repro — svg use (#565 extend) |
| `2cd07c34` | 2026-10-06 22:48 | #577 | docs(diagnostics): reduced repro — mask-image (#565 extend) |
| `9d3af3a9` | 2026-10-06 22:48 | #578 | docs(diagnostics): reduced repro — icon font PUA (#565 extend) |
| `0952a061` | 2026-10-06 22:49 | #568 | test: 25-site real-site list (top 20 + five), holdout swap |
| `1e129237` | 2026-10-06 23:08 | #581 | docs(diagnostics): reduced repros — H13 google logo block + H15 ebay search input |
| `5913252f` | 2026-10-06 23:08 | #566 | fix(renderer): honour object-fit and object-position for images |
| `f117d467` | 2026-10-06 23:15 | #572 | docs(census): technique census top80 (icons/lazy/stacking/bundles/frameworks/bot walls) — no engine code |
| `e14f7f0e` | 2026-10-06 23:16 | #579 | docs(diagnostics): reduced repro — ebay menu behind content, trimmed (#565 extend, H10) |
| `283db90f` | 2026-10-06 23:21 | #582 | docs(diagnostics): missing-text classes — cargurus / autotrader / bringatrailer |
| `908634c8` | 2026-10-06 23:23 | #567 | feat: implement same-document svg use references (W5-D) |
| `f95eb9c8` | 2026-10-06 23:34 | #580 | fix(engine,app): follow a navigation the page's own script starts (H14/H16 check h16_click_nav) |
| `a456ccac` | 2026-10-06 23:40 | #571 | chore: compile-warnings audit (W5-E) and mechanical (B) fixes |
| `a7564057` | 2026-10-07 00:35 | #588 | docs(census): walls-by-vendor table for technique census top80 (follow-up to #572) |

### `a181da05` — 2026-10-07 13:00 ET

`a7564057` → `a181da05` → **/60** **24 → 26** (scorable 23/57 → 25/57), **/75** **26 → 29** (scorable 25/66 → 28/66) → #583, #569, #584, #585, #597, #586, #598, #599, #600.

- Full sha: `a181da0525c355ea65fec05cb33938ad56be3860`
- This tip is the merge of [#600](https://github.com/hiwavebrowser/hiwave-macos/pull/600) (`docs(census): cleanup top100 after #597 (exclusive walls, 429s, retry)`), merged 2026-10-07 11:40 ET (15:40:38Z).
- Full 25-site board at 20261007T170045Z (2026-10-07 13:00 ET). Chrome 148.0.7778.216.
- /60: **24 → 26** (scorable 23/57 → 25/57). /75: **26 → 29** (scorable 25/66 → 28/66). This is a rise, not a drop.
- Per-site: github 0 → 1, wikipedia 1 → 2, autotrader 1 → 2. Everything else unchanged.
- Localisation: github is #598. wikipedia and autotrader are Chrome oracle drift. No other PR credited.

**github 0 → 1.** The blank page at `a7564057` is gone. rustkit now loads (43 rustkit words vs 60 Chrome words, readable ratio 0.7167, loads pass, script_stats threw 0, over_budget 0). This is the expected effect of #598 (atlas/z-github-hydration, merge `7877dd6f`, the `class extends EventTarget` Illegal constructor fix), which is in this window. Post-fix recheck: confirmed on this board.

**wikipedia 1 → 2** is Chrome oracle drift reversing. rustkit words unchanged at 190; Chrome oracle words back from 152 to 178 (the Wiki Loves Monuments CentralNotice banner is no longer in chrome-a), readable ratio 0.7434 → 0.8146, over the 0.80 line. No PR credited.

**autotrader 1 → 2** is Chrome oracle drift. rustkit.png is byte-identical between the two tips (rustkit words 41 both); Chrome words 54 → 51 moved the readable ratio 0.7593 → 0.8039 over the 0.80 line. No PR credited.

First-parent merges on `develop` after `a7564057` through this tip (oldest first):

| Merge | When (ET) | PR | Subject |
|---|---|---:|---|
| `9cdf547e` | 2026-10-07 07:39 | #583 | fix(js,engine): bounded run_jobs drain and config propagation (#574 item 2) |
| `6287b6f9` | 2026-10-07 07:51 | #569 | feat: CSS mask-image (parse + paint) — W5-C |
| `4de8d7cd` | 2026-10-07 08:20 | #584 | fix(js): resolve executor context aliasing with per-executor owned state (#574 item 1) |
| `20e5ceb7` | 2026-10-07 09:31 | #585 | fix(js): resolve console flush recursion via direct Boa evaluation (#574 item 3) |
| `8a12d560` | 2026-10-07 10:03 | #597 | docs(census): proposed top100 pin + prevalence tables (#593) |
| `3307cc64` | 2026-10-07 10:06 | #586 | fix(bindings): reject consuming or cloning Request and Response with locked body stream (#574 item 4) |
| `7877dd6f` | 2026-10-07 10:32 | #598 | fix(bindings): EventTarget is constructible (github.com's landing page ended in its error boundary) |
| `3b5224e0` | 2026-10-07 10:53 | #599 | fix(layout): a border-box flex item with a zero basis grows from its padding and border (H15, ebay's search box) |
| `a181da05` | 2026-10-07 11:40 | #600 | docs(census): cleanup top100 after #597 (exclusive walls, 429s, retry) |

### `7eb35d97` — 2026-10-08 04:30 ET

`a181da05` → `7eb35d97` → **/60** **26 → 26**, **/75** **29 → 29** → #602, #604, #605, #606, #608, #609, #610, #587, #611, #612, #613, #554.

- Full sha: `7eb35d97d19e68d90a7deec7c7dce62233ce8c6a`
- This tip is the merge of [#554](https://github.com/hiwavebrowser/hiwave-macos/pull/554) (`perf(engine): turn tree reuse on by default (Z B0, A2 approved)`), merged 2026-10-08 04:08 ET.
- Board 20261008T083045Z (2026-10-08 04:30 ET). Still the 25-site list, before #617.
- /60: **26**, same as `a181da05`. /75: **29**.
- Per-site: none. The score did not move.
- Localisation: none. There is no measured delta.

First-parent merges on `develop` after `a181da05` through this tip (oldest first):

| Merge | When (ET) | PR | Subject |
|---|---|---:|---|
| `41fb6d0e` | 2026-10-07 16:55 | #602 | fix(layout): percentage heights resolve against a flex item's definite height, and on controls (H15) |
| `f998244c` | 2026-10-07 19:51 | #604 | fix(layout): min-height and max-height resolve rem, em and viewport units |
| `2585d8ef` | 2026-10-07 20:05 | #605 | fix(engine): a style or stylesheet link inside noscript is not a sheet when scripting is on (H14) |
| `dd99c1be` | 2026-10-07 20:22 | #606 | docs(diagnostics): reduced repro, external svg use (census tech 8) |
| `05956fda` | 2026-10-07 21:07 | #608 | fix(layout): an unstretched row flex item keeps its min-height |
| `51bea242` | 2026-10-07 21:08 | #609 | feat(parity-capture): --script-budget-ms for a labelled run at the app's script budget |
| `4ec32c0d` | 2026-10-07 21:08 | #610 | fix(app): the first wheel burst after launch is logged (driver h6 log check) |
| `8b6dd2b4` | 2026-10-07 21:24 | #587 | ci: add blocking js-suites lane for rustkit-js and rustkit-bindings (#574 item 5) |
| `51ad7d4f` | 2026-10-07 22:26 | #611 | fix(css): a content value is its strings, escapes resolved and joined (H17: icon-font glyphs were shown as \f007 text) |
| `82a5a950` | 2026-10-07 22:37 | #612 | fix(layout): an svg or img sized in rem, em, vw, vh, calc() or min() takes that size; min-width and min-height on them (H17 icon sizing) |
| `ce163d8e` | 2026-10-07 22:57 | #613 | fix(layout): a flex container with a rem, em, vh or calc() height aligns, stretches and justifies its items in that height (H17 alignment) |
| `7eb35d97` | 2026-10-08 04:08 | #554 | perf(engine): turn tree reuse on by default (Z B0, A2 approved) |

### `3363c677` — 2026-10-08 13:00 ET

`7eb35d97` → `3363c677` → **/60** **26 → 24**, full **29/78** (first 26-site board after #617) → #615, #616, #614, #617, #618, #620, #623.

- Full sha: `3363c67795e1e89c751800d5181a18ed2d78f8c6`
- This tip is the merge of [#623](https://github.com/hiwavebrowser/hiwave-macos/pull/623) (`docs(diagnostics): #621 table whitespace rows — infobox reduced page`), merged 2026-10-08 12:04 ET.
- Board 20261008T170050Z (2026-10-08 13:00 ET).
- /60: **24** (drop 26 → 24). Full board: **29/78**. This is the first 26-site board after #617. The score table keeps the /75 column; 29 in that column is the full score, and the max is 78.
- Per-site: google 3 → 1. simonwillison appears at 2.
- Localisation: no engine regression PR. Do not blame #617 or #623.

**google 3 → 1** (Trace, 2026-10-13, medium confidence). No engine regression PR. Chrome kept the AI-chips homepage on both tips; only the promo line rotated. At `7eb35d97` rustkit already had the H22 conic/rainbow covering the search row, and the score was still 3. At `3363c677` that block grew, and rustkit showed the classic Google Search / I'm Feeling Lucky homepage instead of the AI chips. Google served rustkit a different variant on top of the old conic bug. #632, in the next window, is the H22 reduce for that conic. Do not blame #617 (the list change) or #623 (docs).

There is no separate #621 first-parent merge. The branch `reduce/621-table-whitespace` is this tip's #623, and that pull request is docs. The parser fix that cites #621 is #624, first-parent in the next window.

First-parent merges on `develop` after `7eb35d97` through this tip (oldest first):

| Merge | When (ET) | PR | Subject |
|---|---|---:|---|
| `d4d82abe` | 2026-10-08 04:59 | #615 | fix(css): a media feature length may be calc(), min(), max(), clamp() or any length unit |
| `18d12588` | 2026-10-08 09:37 | #616 | fix(js,engine,app): a script still running at the budget is stopped at its next host call (ebay hang) |
| `6f6496cd` | 2026-10-08 10:03 | #614 | fix(engine): the root element is body's ancestor in the cascade (.client-js .x, html.dark .x) |
| `baac9807` | 2026-10-08 11:05 | #617 | test: add simonwillison.net/tags/browser-challenge to the real-site list |
| `f08b881a` | 2026-10-08 11:32 | #618 | docs(diagnostics): H19 float-wrap BEFORE evidence, 5 float shapes |
| `a0f07ba5` | 2026-10-08 11:38 | #620 | fix(svg,engine): sprite sheets: `<g>` and nested `<svg>` are containers, vector backgrounds are cut to their box, short background lists repeat |
| `3363c677` | 2026-10-08 12:04 | #623 | docs(diagnostics): #621 table whitespace rows — infobox reduced page |

### `65e1e2f4` — 2026-10-09 04:30 ET

`3363c677` → `65e1e2f4` → **/60** **24 → 26**, full **29/78 → 31/78** (max is 78 after #617) → #628, #624, #619, #631, #632, #630, #625, `2616ec28`, #634.

- Full sha: `65e1e2f4c6f84f883f35ea916249e0ee1f74a8d5`
- This tip is the merge of [#634](https://github.com/hiwavebrowser/hiwave-macos/pull/634) (`fix(engine): one reset for a new document, before its first layout (H27)`), merged 2026-10-08 22:20 ET.
- Board 20261009T083029Z (2026-10-09 04:30 ET).
- /60: **26** (recover 24 → 26). Full board: **31/78**. The score table keeps the /75 column; 31 in that column is the full score, and the max is 78 after #617.
- Per-site: google 1 → 2, wikipedia 2 → 3, simonwillison 2 → 3, autotrader 2 → 1.
- Localisation: recovery of the prior google drop. No new blame.

`2616ec28` is on the first-parent line between #625 and #634. The subject is `probe`. It is not a numbered pull request. Git records two parents; the second parent is `atlas/z-flex-percent-basis-indefinite` (flex percent basis).

First-parent commits on `develop` after `3363c677` through this tip (oldest first):

| Merge | When (ET) | PR | Subject |
|---|---|---:|---|
| `6c9265db` | 2026-10-08 13:08 | #628 | fix(engine,app): the app draws a view only when its frame would differ from the one on screen |
| `da493810` | 2026-10-08 14:19 | #624 | fix(html): whitespace between table rows no longer splits the tbody or leaves the table open (#621) |
| `e845c42e` | 2026-10-08 14:21 | #619 | fix(layout): line boxes beside a float are shortened (H19, Wikipedia thumbnails and infoboxes) |
| `8066f724` | 2026-10-08 14:25 | #631 | fix(app): script budget for live pages is 15 s, down from 60 s |
| `d357ee92` | 2026-10-08 14:59 | #632 | docs(diagnostics): H22 google search-box conic gradient — reduced page |
| `ca45f458` | 2026-10-08 15:24 | #630 | feat(layout): table layout, first slice: grid, auto column widths, spans, spacing (#626) |
| `c9d7ba1b` | 2026-10-08 15:27 | #625 | fix(js,engine): a script stopped at the budget says so in the log, with how long it ran |
| `2616ec28` | 2026-10-08 15:39 | — | probe |
| `65e1e2f4` | 2026-10-08 22:20 | #634 | fix(engine): one reset for a new document, before its first layout (H27) |

### `cea4c0c6` — 2026-10-09 13:36 ET

`65e1e2f4` → `cea4c0c6` → **/60** **26 → 26**, full **31/78 → 31/78** (max is 78 after #617) → #635, #636, #637, #638, #639, #640, #645, #646, #633.

- Full sha: `cea4c0c6662da257679d3043c2813775f907ed4b`
- This tip is the merge of [#633](https://github.com/hiwavebrowser/hiwave-macos/pull/633) (`fix(paint): opacity on boxes, as a group layer (H22)`), merged 2026-10-09 12:50 ET.
- Board 20261009T173658Z (2026-10-09 13:36 ET).
- /60: **26**. Full board: **31/78**. The score table keeps the /75 column; 31 in that column is the full score, and the max is 78 after #617.
- Per-site: google 2 → 3, wikipedia 3 → 2.
- Localisation: no drop.

First-parent merges on `develop` after `65e1e2f4` through this tip (oldest first):

| Merge | When (ET) | PR | Subject |
|---|---|---:|---|
| `639220fe` | 2026-10-09 09:14 | #635 | fix(renderer): a frame during which the glyph atlas starts over is built again, on an emptied atlas (H23) |
| `b5fcd677` | 2026-10-09 09:14 | #636 | fix(cascade): an ancestor's attributes and :not() are tested against that ancestor (microsoft.com blank under #633) |
| `f54103d9` | 2026-10-09 09:14 | #637 | feat(parity-capture): --live-ms turns the app's live loop after a load; trace the DOM write that dirtied the page (tool) |
| `ccf6a7b3` | 2026-10-09 11:35 | #638 | fix(engine): white space beside a block-level image or control makes no line (wikipedia logo; clears #622's 14px) |
| `a973287e` | 2026-10-09 11:35 | #639 | fix(bindings): an IntersectionObserver's first entry waits for the layout a DOM write is owed (found on github.com; does not stop its loop) |
| `e69a375b` | 2026-10-09 11:35 | #640 | fix(bindings): a live turn runs one round of animation frames, not one per 16 ms it took |
| `2c0d63ca` | 2026-10-09 11:35 | #645 | fix(bindings): canvas.getContext and media element methods exist (github.com remounted its page every turn) |
| `230f6c44` | 2026-10-09 12:50 | #646 | Fix standalone engine test compilation and ancestor-slice regression fixture |
| `cea4c0c6` | 2026-10-09 12:50 | #633 | fix(paint): opacity on boxes, as a group layer (H22) |

### `16f97285` — 2026-10-09 17:29 ET

`cea4c0c6` → `16f97285` → **/60** **26 → 27**, full **31/78 → 32/78** (max is 78 after #617) → #651, #649, #648, #658, #653, #650, #655, #657, #659.

- Full sha: `16f97285eba005640e1feb030ba5fce9a5beaadb`
- This tip is the merge of [#659](https://github.com/hiwavebrowser/hiwave-macos/pull/659) (`fix: a replaced box with a height and a ratio takes its width from them in flex and the width estimators (H26c)`), merged 2026-10-09 17:27 ET.
- Board 20261009T212952Z (2026-10-09 17:29 ET).
- /60: **27** (rise 26 → 27). Full board: **32/78**. The score table keeps the /75 column; 32 in that column is the full score, and the max is 78 after #617.
- Per-site: wikipedia 2 → 3.
- Localisation: rise. No other site moved.

First-parent merges on `develop` after `cea4c0c6` through this tip (oldest first):

| Merge | When (ET) | PR | Subject |
|---|---|---:|---|
| `f13f0414` | 2026-10-09 13:48 | #651 | docs: table AFTER evidence with cell rects (reduce_ pages; caption-side bottom, UA p margin) |
| `6b7f0b3d` | 2026-10-09 13:48 | #649 | docs: H24 ebay large icons (CSS-sized svg use paints in 300x150 viewport) |
| `c330c43e` | 2026-10-09 14:46 | #648 | feat(tools): element_diff — per-element box comparison against Chrome, with overlay |
| `c63b57bd` | 2026-10-09 15:17 | #658 | fix: a CSS-sized svg with a use paints at its box size, not 300x150 (H24) |
| `9d95aabf` | 2026-10-09 15:48 | #653 | test(layout): sizing equivalence matrix (W7-A) + 7 unit-resolution fixes |
| `194e48e7` | 2026-10-09 15:48 | #650 | fix(svg): flatten path curves to a device-pixel tolerance and sweep x-disjoint contours apart (W7-D) |
| `642c9c9f` | 2026-10-09 17:26 | #655 | feat(engine): log why each relayout happened (W7-C) |
| `bffcfa3f` | 2026-10-09 17:26 | #657 | fix: auto margins on flex items take the free space (H25) |
| `16f97285` | 2026-10-09 17:27 | #659 | fix: a replaced box with a height and a ratio takes its width from them in flex and the width estimators (H26c) |

### `81ef8b34` — 2026-10-12 04:30 ET

`16f97285` → `81ef8b34` → **/60** **27 → 26**, full **32/78 → 31/78** (max is 78 after #617) → #647, #652, #654.

- Full sha: `81ef8b34c716dece7804375221d78f3a9dd9083b`
- This tip is the merge of [#654](https://github.com/hiwavebrowser/hiwave-macos/pull/654) (`docs: H26 reddit top defects with reduced pages (grid order/fixed, grid in flex, slot/logo)`), merged 2026-10-09 20:02 ET. The board is the next morning run after the weekend, 2026-10-12 04:30 ET.
- Board 20261012T083026Z (2026-10-12 04:30 ET).
- /60: **26** (drop 27 → 26). Full board: **31/78**. The score table keeps the /75 column; 31 in that column is the full score, and the max is 78 after #617.
- Per-site: google 3 → 2.
- Localisation: no engine PR.

**google 3 → 2** (Trace, final, high confidence). No engine PR. chrome-a and chrome-b are more stable on this tip than on the previous one. rustkit drew the classic homepage, with no doodle and no chips. Chrome had the AI chips. The missing words are chip labels that the classic page does not have. This flip has gone both ways on earlier tips. #647, #652, and #654 are unrelated to Google.

First-parent merges on `develop` after `16f97285` through this tip (oldest first):

| Merge | When (ET) | PR | Subject |
|---|---|---:|---|
| `e7f40591` | 2026-10-09 20:01 | #647 | docs: H25 simonwillison gutter (column-flex item auto margins ignored) |
| `9771bda0` | 2026-10-09 20:02 | #652 | docs: thread ownership design for responsiveness and Phase X (W7-E) |
| `81ef8b34` | 2026-10-09 20:02 | #654 | docs: H26 reddit top defects with reduced pages (grid order/fixed, grid in flex, slot/logo) |

### `696f9921` — 2026-10-12 13:00 ET

`81ef8b34` → `696f9921` → **/60** **26 → 26**, full **31/78 → 31/78** (max is 78 after #617) → #662, #667, #661.

- Full sha: `696f9921e4f1d92903e1807df3b14e2a53999ac8`
- This tip is the merge of [#661](https://github.com/hiwavebrowser/hiwave-macos/pull/661) (`fix: receipt.py prints no host paths`), merged 2026-10-12 11:29 ET.
- Board 20261012T170047Z (2026-10-12 13:00 ET).
- /60: **26**. Full board: **31/78**. The score table keeps the /75 column; 31 in that column is the full score, and the max is 78 after #617.
- Per-site: google 2 → 3, wikipedia 3 → 2.
- Localisation: no drop.

First-parent merges on `develop` after `81ef8b34` through this tip (oldest first):

| Merge | When (ET) | PR | Subject |
|---|---|---:|---|
| `5feb459a` | 2026-10-12 08:51 | #662 | fix: svg paths and strokes scroll with the page (polygons and lines go through the transform stack) |
| `d6b449f6` | 2026-10-12 11:03 | #667 | fix: a page that asked to leave is not kept waiting for images its scripts added (I0a) |
| `696f9921` | 2026-10-12 11:29 | #661 | fix: receipt.py prints no host paths |

### `bbd92964` — 2026-10-13 04:58 ET

`696f9921` → `bbd92964` → **/60** **26 → 27**, full **31/78 → 32/78** (max is 78 after #617) → #669, #671, #672, #670, #673, #680, #683, #682.

- Full sha: `bbd92964176894f9df9a1bb2c8dd0fc0d72295e1`
- This tip is the merge of [#682](https://github.com/hiwavebrowser/hiwave-macos/pull/682) (`docs: the no-check label parks a pull request`), merged 2026-10-12 23:51 ET.
- Board 20261013T085801Z (2026-10-13 04:58 ET).
- /60: **27** (rise 26 → 27). Full board: **32/78**. The score table keeps the /75 column; 32 in that column is the full score, and the max is 78 after #617.
- Per-site: walmart 1 → 2. google's rustkit capture is identical across this step.
- Localisation: rise, not a drop. Possible Chrome settling, and maybe #683. Low–medium confidence.

**walmart 1 → 2** (Trace, final, low–medium). This is a rise, not a drop. google's rustkit capture is identical across this step. Possible Chrome settling, and maybe #683 (a brotli response is decoded even when the engine did not ask for it).

First-parent merges on `develop` after `696f9921` through this tip (oldest first):

| Merge | When (ET) | PR | Subject |
|---|---|---:|---|
| `a9a0cdbe` | 2026-10-12 17:49 | #669 | test: nested layout matrix, 609 pages of one layout mode inside another against Chromium (D1) |
| `3f4f8c37` | 2026-10-12 17:49 | #671 | feat: shadow DOM renders through the flat tree (S0 slice 1) |
| `75f32786` | 2026-10-12 18:21 | #672 | fix: width: fit-content on a block is its shrink-to-fit width, not 0 (D1) |
| `2b75e93b` | 2026-10-12 19:23 | #670 | fix: gap with two values sets the row gap and the column gap (D1) |
| `f622466d` | 2026-10-12 20:25 | #673 | fix: an empty block sized only by aspect-ratio keeps its box (D1) |
| `fe75d982` | 2026-10-12 23:51 | #680 | fix: the GPU test guard's limit is on one test's hold, not on the queue (false failures in loaded full runs) |
| `96a8f36e` | 2026-10-12 23:51 | #683 | fix: a brotli response is decoded even though we did not ask for it |
| `bbd92964` | 2026-10-12 23:51 | #682 | docs: the no-check label parks a pull request |

## Append a tip

Copy this block under **Entries**, fill it, and add one row to the score table. Keep oldest-first order.

```
### `<shortsha>` — <YYYY-MM-DD HH:MM ET>

`<previous-shortsha>` → `<shortsha>` → **/60** <n or TBD>, **/75** <n, —, or TBD> → #<pr>, #<pr>, …

- Full sha: `<40-char sha>`
- This tip is the merge of #<pr> (`<subject>`).
- Per-site notes: <site and what moved, or "none">
- Localisation: <PR blamed, or "oracle drift" with the Chrome surface and whether rustkit.png matched. "none" when there is no measured delta.>
```

| Merge | When (ET) | PR | Subject |
|---|---|---:|---|
| `<sha>` | | # | |
