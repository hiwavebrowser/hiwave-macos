# Board ledger

Trace's standing board ledger for Atlas / ZeuzGb. It records the real-site board score at `develop` tips so a later drop can be read as: develop sha, board numbers on that sha, pull requests merged since the previous tip.

On a drop of the /60 score or the /75 score, Trace checks Chrome oracle drift first. An engine capture that is byte-identical to the previous tip is oracle drift, and no merge is blamed. When the engine capture moved, Trace bisects the pull requests listed on that row.

A score is entered only when a board run measured it on that sha. Anything unmeasured stays **TBD**.

**Order:** oldest first. Append a new tip at the bottom. Do not insert above the seed.

**Denominators**

- **/60** is the original 20 sites (`websuite/realsite-top20.json`).
- **/75** is all 25 sites. [PR #568](https://github.com/hiwavebrowser/hiwave-macos/pull/568) (Atlas, open at this seed, not an ancestor of `50e77c83`) grows the list to 25. The original 20 stay comparable on /60, and the full list is scored on /75. Both denominators are recorded together starting with the 4:30 AM run (`a7564057`, **26/75**).
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
