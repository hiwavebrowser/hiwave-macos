# Real-site board: analysis and plan (2026-09-25)

Written for Pete's question: *what do the live captures say, do we have expected results, which results are surprising, are we getting better, and what is the best route to parity?*

**Data used.**
- 30 run dirs under `trench/realsite/runs/` (2026-09-24 02:22Z → 2026-09-25 18:10Z), not committed.
  - 17 are full-board runs (20 sites). Only these are used for trends.
  - 13 are A/B or subset runs (2–7 sites), used only to attribute causes.
  - Engine logs exist for the 9 full runs since 0310Z.
- A new Chrome-only structure probe (`trench/tools/structure_probe.mjs`: pinned CfT 148.0.7778.216, 1280×800, run 2026-09-25 ~19:05Z) inventoried every site.
- RustKit CSS support was read from the `apply_style_property` parse arms on develop `6b73004`.
- Scripts: `trench/tools/analysis/` (`trend.py`, `matrix.py`, `timeline.py`, `anatomy.py`, `join.py`).

**Noise bounds.**
- Full-board points: mean 12.9, sd 1.5, range 10–15 across 17 runs.
- 25 LOADS flips between consecutive full runs: apple 6, netflix 4, yahoo 4, weather 3, wikipedia 3, microsoft 2 (these three on identical code), and others.
- A ±1 in one run is not a signal. A ±2 held for 3 runs is.

Spot-checked by Atlas before commit:
- x's oracle text is Chrome's "Access to x.com was denied … HTTP ERROR 403".
- linkedin's log shows `Failed to parse external stylesheet … ParseError("Unexpected end of input")` for its static.licdn.com sheet.

## 1. Top findings

1. **Cascade got about 6× faster, and the wall moved to layout.**
   - Summed per-site cascade time: 108.9 s → 17.1 s (0310Z → 1810Z).
   - Summed layout+paint: 46.8 s → 84.2 s.
   - Layout-bound now: netflix 27.4 s, github 17.9 s, cnn 13.5 s, wikipedia 11.4 s, facebook 10.0 s.
   - Sites killed at 30 s: 6 → 3.
2. **Points barely moved (12 → 14–15, sd 1.5), and two of today's 14 are artifacts.**
   - x's LOOKS RIGHT (17 of 17 runs) is scored against Chrome's own 403 page.
   - google's LOOKS RIGHT passes at 10–11% on a visibly broken page: a giant blue conic-gradient pill and an off-centre logo. Chrome's frame is only 3.8% non-background.
   - **Honest board today: 12/60.**
3. **CSS coverage is the biggest cheap lever, and it is mostly missing parse arms, not missing engines.**
   - Engine code exists: `Float`, `ObjectFit`, `Position::Sticky`, and SVG `visibility`.
   - Never parsed from CSS:
     - `float`, `clear`, `object-fit`, and `visibility` on HTML boxes.
     - `list-style*` and every logical property.
     - `filter`, `text-shadow`, `clip-path`/`mask`, and `grid-area`/`grid-template-areas`.
     - `display: table*|list-item|contents|flow-root`.
   - facebook and instagram: 23% of all declarations use a property RustKit drops.
   - `visibility` is dropped on 16 of 20 sites, `float` on 16, `list-style*` on 17, logical margins on 13.
4. **Whole stylesheets are thrown away on one parse error.**
   - linkedin's only sheet (341 KB) hits `ParseError("Unexpected end of input")`, so the page renders unstyled.
   - x (386 KB), yahoo (585 KB) and weather (296 KB) each lose a big sheet the same way.
5. **Images fail on dispatch and sizing, not on codecs.**
   - 13 of 14 `Unknown image format` failures are SVGs served without `.svg`: microsoft's 12 quick-link icons and linkedin's logo.
   - Unsized SVGs paint at the wrong size: bing's giant magnifier, netflix's 1280-px wordmark, weather's giant blue squares, google's misplaced logo.
   - facebook's "blank frame" (1.0% vs the 2% line) is not blank. Its text and form are there; its webp hero and logo fill are missing.

## 2. Are we getting better? (trend)

Full-board points, in run order: 10 12 13 14 13 11 14 12 11 15 12 13 12 14 15 15 14.
The per-site L/R/V history is produced by `trench/tools/analysis/matrix.py`.

| run | pts | LOADS | 30 s kills | Σ READABLE ratio | mean LOOKS diff (loaded) | median RustKit s (finishing sites) |
|---|---|---|---|---|---|---|
| 09/24 0301 (baseline) | 12 | 8 | 7 | 2.29 | 36.3% | 13.7 |
| 09/24 0417 | 14 | 10 | 4 | 3.64 | 35.2% | 7.2 |
| 09/25 0350 | 15 | 11 | 3 | 3.77 | 38.4% | 11.8 |
| 09/25 1500 | 15 | 10 | 3 | 4.57 | 37.6% | 6.4 |
| 09/25 1810 | 14 | 10 | 3 | 3.46* | 37.2% | 5.5 |

\* Oracle drift. Chrome showed Wikipedia's JS fundraising banner, which moved the article out of the first viewport: 57% → 25%.

**Verdict.** The engine is faster: kills 7 → 3, median finish 13.7 s → 5.5 s, readable text up about 50% at peak. The score hasn't followed. Each speedup was spent reaching the next wall (cascade, then layout and network stalls), and most failing sites fail two checks for unrelated reasons. The 7-days-no-point stop condition is not triggered.

## 3. Where each site's 30 s goes

Phase seconds from engine-log intervals; fetches overlap, so these are approximate. `*` = killed at 30 s, followed by the phase it was killed in.

| site | 0310Z | then | 1810Z | now |
|---|---|---|---|---|
| github | 30* cascade | cascade 25.5 | 30* layout | layout 17.9, cascade 6.3 |
| netflix | 4.5 | — | 30* layout | layout 27.4 |
| cnn | 30* cascade | cascade 29.0 | 28.0 | layout 13.5, cascade 6.9, img 3.7 |
| apple | 30* script | cascade 12.8, layout 11.3 | 30* net:css | one stylesheet fetch stalled 29.1 s |
| wikipedia | 30* cascade | cascade 14.0 | 15.1 | layout 11.4 |
| facebook | 30* cascade | css 11.2, doc 9.0, fonts 7.9 | 15.4 | layout 10.0 |
| microsoft | 6.1 | — | 11.1 | img 3.7, fonts 3.7 (hung in Boa or an image fetch in 5 of today's 14 captures) |
| linkedin | 30* layout | layout 14.2 | 2.4 | — |
| youtube | 18.9 | css 8.5 | 5.1 | fonts 3.7 (then paints an empty app shell) |

Kills by phase across the 9 logged full runs: layout 15, cascade 13 (all before #256–#258), script 4, network fetch never returned 4. Cascade no longer kills anything; layout and network stalls are the LOADS wall now.

## 4. Page structure (Chrome-side inventory)

| site | DOM | vp elems | vp flex/grid | inline SVG | img (lazy) | JS MB | CSS KB | decls dropped | server-HTML share of first-viewport words |
|---|---|---|---|---|---|---|---|---|---|
| google | 345 | 77 | 17/0 | 11 | 6 | 2.0 | 126 | 2.9% | 92% |
| youtube | 2032 | 155 | 53/0 | 85 | 7 | 14.4 | 4279 | 2.2% | 0% |
| facebook | 385 | 169 | 96/0 | 2 | 1 | 4.9 | 2274 | 22.8% | 91% |
| instagram | 518 | 181 | 94/0 | 3 | 1 | 7.3 | 1236 | 23.2% | 0% |
| wikipedia | 4169 | 432 | 71/2 (+4 table) | 0 | 12 (9) | 1.2 | 389 | 13.0% | 90% |
| linkedin | 811 | 79 | 20/0 | 11 | 8 | 1.8 | 342 | 1.2% | 100% |
| yahoo | 3460 | 881 | 266/11 | 99 | 209 (202) | 26.6 | 917 | 3.6% | 29% |
| bing | 698 | 204 | 18/2 | 10 | 31 (3) | 9.3 | 480 | 5.0% | 0% |
| microsoft | 1351 | 86 | 32/1 | 4 | 46 (26) | 9.7 | 699 | 2.2% | 13% (248 shadow roots, 290 custom elements) |
| apple | 1952 | 796 | 113/2 | 88 | 51 | 1.1 | 690 | 12.4% | 100% |
| netflix | 1086 | 106 | 31/0 | 13 | 4 | 9.4 | 556 | 3.5% | 100% |
| github | 1809 | 455 | 139/7 | 116 | 24 (24) | 6.7 | 2434 | 10.0% | 72% |
| cnn | 5395 | 292 | 67/1 | 64 | 119 (101) | 13.0 | 3034 | 7.1% | 43% |
| weather | 2610 | 118 | 34/1 | 123 | 27 (25) | 19.3 | 482 | 7.2% | 24% |
| amazon, reddit, x, chatgpt, ebay, nytimes | 9–39 | — | — | — | — | — | — | — | the oracle was served a block or challenge page |

What the inventory says:
- **Flex matters more than grid.** Flex appears in the first viewport on 14 sites (17–266 containers); grid on 8 (at most 11 containers).
- **For 8 sites, READABLE is a CSS/layout/perf problem, not a JS problem.** They carry at least 90% of their first-viewport words in server HTML (github 72%).
- **JS-only sites:** youtube, instagram and bing at 0%; microsoft 13%; weather 24%; yahoo 29%. microsoft also needs Shadow DOM.
- **Image formats:** webp 86, avif 310 (yahoo and microsoft), svg 68. There's no AVIF decoder; check RustKit's `Accept` header before writing one.
- **Modern CSS in the wild:**
  - `@container`: weather 199, cnn 96.
  - `@supports`: youtube 444.
  - `@property`: facebook 298.
  - `:has()`: cnn 1261.
  - `var()`: github 20k.
  - `@layer` is rare.

### RustKit property coverage (develop 6b73004)

Properties with no `apply_style_property` arm. A literal search across RustKit crates found none handled elsewhere, except `fill`/`stroke` as SVG attributes.

| property | decls (20 sites) | sites | engine already present? |
|---|---|---|---|
| logical margin/padding/inset | ~7,100 | 13 | none needed: aliases in ltr |
| logical border colour/width/radii | ~4,300 | 6 | aliases |
| font-stretch | 1,286 | 6 | Core Text supports width; not wired |
| fill/stroke on inline SVG via CSS | 1,181 | 15 | SVG painter has fill/stroke |
| visibility | 676 | 16 | SVG only; HTML boxes paint hidden content |
| filter/backdrop-filter | 844 | 17/13 | no |
| float/clear | 605/145 | 16/14 | yes (layout floats); never set from CSS |
| mask*/clip-path/clip | ~780 | 13 | no |
| list-style* | 370 | 17 | no |
| text-shadow | 260 | 10 | no |
| grid-area/grid-template-areas | 158+ | 8 | placement yes, areas no |
| object-fit | 152 | 14 | yes (paint path); never set from CSS |
| display table*/list-item/contents/flow-root | contents: 5 sites | — | `parse_display` knows 8 values and ignores the rest |

## 5. Visual diff anatomy (1810Z)

| site | diff | made of |
|---|---|---|
| bing | 93% | Chrome: full-bleed photo plus JS news cards. RustKit: grey plus a giant unsized magnifier SVG. |
| weather | 59% | Unsized-SVG giant squares; fixed sidebar missing; dropped stylesheet. |
| microsoft | 51% | Hero matches. Nav and quick links render as stacked link lists (web components, hidden menus). 12 SVG icons fail to decode. |
| cnn | 45% | Chrome consent modal and ad band. RustKit band heights wrong; blank ad slot. |
| wikipedia | 41% | Chrome banner shifts the article 268 px. RustKit: header grid and sidebars missing, language dropdown painted open, no table-of-contents column. |
| linkedin | 20% | Fully unstyled page (sheet dropped). READABLE is still 97%. |
| instagram | 16% | JS-only. The near-pass is shared white space. |

Classes: unsized/unstyled SVG on 5 sites, hidden content painted on 3+, dropped stylesheet on 4, JS-built content on 6.

## 6. Expected vs actual

| site | actual | expected now | note |
|---|---|---|---|
| google | 3 | 2 | LOOKS RIGHT unearned |
| youtube | 0 | 0 | JS-only |
| facebook | 0 | 1–2 | Surprise: text renders, frame scored "blank" |
| instagram | 1 | 1 | |
| wikipedia | 1 | 2 | layout + oracle banner |
| amazon | 0 | 0 | AWS WAF for RustKit only |
| reddit | 0 | unscorable | Chrome also challenged |
| x | 2 | 1–2 once the oracle is real | Surprise: HeadlessChrome UA blocked; verified |
| linkedin | 2 | 3 | Surprise: sheet dropped |
| yahoo | 1 | 1 | |
| bing | 1 | 1 | |
| chatgpt/ebay/nytimes | 0 | unscorable | Chrome also blocked |
| microsoft | 1 | 1 | flaky |
| apple | 0 | 2 | Surprise: 29 s stylesheet stall; READABLE 100% when it loads |
| netflix | 0 | 2 | Surprise: lost LOADS at 0400Z (#254, hypothesis); layout 27 s; 100% server text |
| github | 0 | 1–2 | layout-bound |
| cnn | 1 | 1 | |
| weather | 1 | 1 | |

The honest ceiling from this seat is 48/60, because four sites block Chrome itself.

## 7. Roadmap

**Phase A: honest board (hub tooling, no engine risk).**
- A1. The oracle identifies as the Chrome it is: headed launch, or drop the `HeadlessChrome` token.
- A2. `oracle_blocked` detection; report `n/48`.
- A3. LOOKS RIGHT also requires the diff inside the union of content pixels to be ≤ 35% (proposal).
- A4. Best-of-2 RustKit captures for LOADS.
- A5. Shadow READABLE/LOOKS scores when LOADS fails.
- A6. Append a `trend.csv` every run; run the structure probe weekly.
- A7. Oracle-drift annotations (banners, consent modals).

**Phase B: engine.**

| # | item | sites | expected pts | cost/risk |
|---|---|---|---|---|
| B1 | CSS error recovery per rule, not per sheet | linkedin, x, yahoo, weather | +1 | S / low |
| B2 | per-subresource fetch deadline + total budget | apple, microsoft, cnn | +2 | S / low |
| B3 | parse-arm batch 1 (logical aliases, float/clear, object-fit, visibility, list-style, display flow-root/contents/list-item, clip) | 13–17 each | +1–3 | S / low |
| B4 | SVG images: sniff type, intrinsic sizing, CSS fill/currentColor | 7 sites | +1–2 | M / low |
| B5 | flex layout memo + grid sizing profile | netflix, github, cnn, wikipedia, facebook | +3–5 | M–L / medium |
| B6 | script watchdog (architecture) | microsoft, yahoo, weather | +0–1; prerequisite for B7 | M–L |
| B7 | JS DOM wiring, Shadow DOM, custom elements | 7 JS sites | up to +7 READABLE; Boa vs 14–27 MB bundles | XL / high |
| B8 | challenge walls (no evasion) | amazon | +1–3 | L |
| B9 | batch 2: filter, text-shadow, mask/clip-path, grid-areas, table, font-stretch, AVIF | polish | diffs, few points | M |

Order: A1–A6 first, then B1 + B2 + B3, then B4, then B5. B6/B7 is the architecture track. Expected after A and B1–B5: about 20–24 honest points out of 48 (±3).

## 8. Grok bot capturing per update

Not for the capture; yes for the review.
- RustKit's headless path is `cfg(all(target_os="macos", feature="headless"))`, text shaping is Core Text, the non-mac/non-Windows paths are stubs, and CI's only ubuntu job never builds RustKit.
- LOADS is wall-clock, so machine speed moves it.
- Bot walls depend on IP and client, as the x and reddit results from this Mac show.

Plan: a post-merge board run on this Mac (launchd watching `origin/develop`, serialized with the trench lock), appending `trend.csv`. Grok reviews `trend.csv` and the side-by-sides and flags regressions beyond the noise bounds.

## 9. Decisions for Pete
1. **Oracle identity:** headed Chrome (recommended) or drop the `HeadlessChrome` UA token. Neither touches `navigator.webdriver` or solves challenges.
2. **Scoring rule changes:** the LOOKS RIGHT rule (A3) and best-of-2 LOADS (A4), with old and new numbers restated in BASELINE.
3. **Exit metric:** 60/60 is unreachable from this seat (48 scorable). Change it to points/scorable, or keep it and accept that four sites stay blocked.
