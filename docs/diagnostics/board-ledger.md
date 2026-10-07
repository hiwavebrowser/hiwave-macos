# Board ledger

Trace's standing board ledger for Atlas / ZeuzGb. It records the real-site board score at `develop` tips so a later drop can be read as: develop sha, board numbers on that sha, pull requests merged since the previous tip.

On a drop of the /60 score or the /75 score, Trace checks Chrome oracle drift first. An engine capture that is byte-identical to the previous tip is oracle drift, and no merge is blamed. When the engine capture moved, Trace bisects the pull requests listed on that row.

A score is entered only when a board run measured it on that sha. Anything unmeasured stays **TBD**.

**Order:** oldest first. Append a new tip at the bottom. Do not insert above the seed.

**Denominators**

- **/60** is the original 20 sites (`websuite/realsite-top20.json`).
- **/75** is all 25 sites. [PR #568](https://github.com/hiwavebrowser/hiwave-macos/pull/568) (Atlas, open at this seed, not an ancestor of `50e77c83`) grows the list to 25. The original 20 stay comparable on /60, and the full list is scored on /75. Both denominators are recorded together starting with the 4:30 AM run. No /75 number exists in this file yet.
- #568's body also records a one-load reading of the five new sites on `481db9cb` (3/15). That reading is not a board score and is not a row below.

Seeded 2026-10-06 ET. The 27 and 26 below are that night's record of the github drop. They are not restated in a commit message on these shas.

## Score table

| Tip | Date (ET) | /60 | /75 | Per-site | Localisation |
|---|---|---:|---:|---|---|
| [`24185512`](#24185512--2026-10-05-1233-et) | 2026-10-05 12:33 | 27 | — | — | Before-side of the github drop. |
| [`e3b833fc`](#e3b833fc--2026-10-06-0418-et) | 2026-10-06 04:18 | 26 | — | github | Oracle drift. Do not blame #550. |
| [`50e77c83`](#50e77c83--2026-10-06-2036-et) | 2026-10-06 20:36 | TBD | TBD | TBD | No board rerun on this tip. |
| _next_ | | TBD | TBD | | |
| _next_ | | TBD | TBD | | |
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

`e3b833fc` → `50e77c83` → **/60 TBD, /75 TBD** → #551, #552, #543, #528, #553, #555, #557, #556, #562, #564, #561, #559, #477.

- Full sha: `50e77c838e5dbf87070a54562d2924926256d1af`
- Develop tip as of the 2026-10-06 ET seed. This tip is the merge of [#477](https://github.com/hiwavebrowser/hiwave-macos/pull/477) (`test: cover image loader, h2 header strip, calc shorthand, and subresource budget survivors`).
- Earlier that evening, [#564](https://github.com/hiwavebrowser/hiwave-macos/pull/564), [#561](https://github.com/hiwavebrowser/hiwave-macos/pull/561), and [#559](https://github.com/hiwavebrowser/hiwave-macos/pull/559) landed. The Mac was free again after those. No board rerun has been entered for this tip, including none after the 25-site change.
- /60: **TBD**. /75: **TBD** (the 4:30 AM run is the first one that records both, and only once #568's list is what the board runs).
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
