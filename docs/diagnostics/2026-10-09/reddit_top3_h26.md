# H26: www.reddit.com home, top defects (region by region) and reduced pages

Docs only, no engine code. Site: `https://www.reddit.com/` (logged out home).

- **Measured:** develop `f54103d9`, RustKit `parity-capture --profile parity`, 1280x800, DPR 1 for the reduced pages and the live page.
- **Oracle:** pinned Chrome for Testing 148.0.7778.216, headless, same viewport.
- **Live load:** reddit first answers with a JS challenge page (an inline script that fills a form and calls `requestSubmit()`); parity-capture does not follow that submit, so a plain `--url https://www.reddit.com/` shows only the spinner interstitial. For the layout work the challenge-solved URL that Chromium reached was given to RustKit (`--url` with the `?solution=...&js_challenge=1&jsc_token=...` query it ended on), which renders the real page.
- **Frames:** the two full frames (Chromium and RustKit, 1280x800) are kept in the private renders repo, not here; the region table below is from them and from DOM rects.

## Live page, region by region (Chromium vs RustKit)

| Region | Chromium 148 | RustKit `f54103d9` |
|---|---|---|
| `div.grid-container` | 0,56 1280 wide, columns `315px 965px` | 0,56 1280 wide |
| `#subgrid-container` (feed column) | 315,56 965 wide (`m:col-start-2`, `order-2`) | **0,56 1120 wide** (column 2 lost) |
| `#left-sidebar-container` (`position: fixed; top: 56px; order-first`) | **0,56 315x744**, "Join the most real place" panel at the left edge | **315,56 1120 wide**: the login panel is painted over the middle of the feed |
| `div.main-container` | 339,56 917 wide, two columns `577px 316px` | 24,56 1072 wide, **one column** |
| `#right-sidebar-container` ("Popular communities") | beside the feed | **24,1726 316x743, below the feed** |
| `#reddit-logo` svg (`class="h-[22px]"`, `viewBox 0 0 514 149`) | 24,17 **75.89x22** | 24,17 **0x22** (wordmark not painted) |
| header search / Sign Up / Log In | search 240-688 px, two buttons at the right | search at x=40-485, striped/garbled boxes where the buttons and menu are (two `rounded_rect` ops 40px tall and **2019px / 1551px wide**) |
| sort bar text | "Best", "Everywhere" only | extra text "Open sort options", "Everywhere", "Change view" painted, overlapping the first post |

In the RustKit frame the post cards, thumbnails and images render, but sit in the wrong columns.

## The three defects, each with a reduced page

### H26a: `order` drops a grid item's explicit column, and a `position: fixed` child is laid out as a grid item
`reddit_grid_order_fixed_h26a.html`. Reddit's shell is `grid-template-columns: 315px 965px` with the feed `grid-column-start: 2; order: 2` and the left sidebar `position: fixed; order: -9999` as its sibling.

| 1280x800 | Chromium 148 | RustKit `f54103d9` |
|---|---|---|
| `#g` | 0,0 1280x100 | 0,0 1280x100 |
| `#sub` | **315,0 965x100** | **0,0 315x100** |
| `#side` | **0,56 315x18** (out of flow) | **315,0 315x100** |
| pixel (100,70) | **255,204,153** (side) | **153,204,255** (sub) |
| pixel (600,70) | 153,204,255 | 255,204,153 |

Isolating variants (RustKit): with no `order` on either element `#sub` is correct (315,0 965x100) but `#side` is still a grid item (0,100 315x18, `#g` height 118 instead of 100, `top: 56px` ignored); with `order` on only `#sub` or only `#side`, `#sub` falls to 0,0 315x100. Chromium is the same in all variants as the table above (`#side` 0,56; `#sub` 315,0).

### H26b: a grid container that is a flex item loses its columns or its width
`reddit_grid_in_flex_h26b.html`. The feed's `div.main-container` (`display: grid`, `flex: 1`) sits in a flex column.

| 1280x800 | Chromium 148 | RustKit `f54103d9` |
|---|---|---|
| `#x` (flex row, 700px) | 0,0 700x60 | 0,0 700x**120** |
| `#m` (auto-width grid, `minmax(0,756px) minmax(0,316px)`, gap 24) | **0,0 700x60** | **0,0 46.2x120** |
| `#a` / `#b` | 0,0 360x60 / 384,0 316x60 | 0,0 46.2x60 / **0,60** 46.2x60 (stacked) |
| pixel (100,40) | 153,204,255 | **255,255,255** |
| pixel (620,40) | 255,204,153 | **255,255,255** |

Variants (RustKit): same grid in a block parent is two columns (0,0 467x60 and 491,0 209x60, see H26d); with `px` tracks (`400px 276px`) or `1fr` tracks as a flex item the width is still 46.2; with an explicit `width: 700px` the width is right but the tracks collapse to one column (`#a` 0,0 700x60, `#b` 0,60). So the grid's min/max-content and track definition are not used when its parent is a flex container. On the live page this is the single feed column with "Popular communities" pushed below the feed.

### H26c: unassigned `<slot>` content is painted, and an svg with only `viewBox` + CSS height is 16px wide in a flex item
`reddit_slot_logo_h26c.html`.

| 1280x800 | Chromium 148 | RustKit `f54103d9` |
|---|---|---|
| `#logo` svg (`viewBox 0 0 514 149`, `height: 22px`, in `a { display: flex }`) | **24,17 75.89x22** | **24,17 16x22** |
| pixel (60,28) | **255,68,0** (logo) | **255,255,255** |
| `#dd` custom element with a shadow root `<slot name="sel">` | 123.89,20 **28.02x16** | 64,12 **110.51x32** |
| `#tip` (slotted into a slot the shadow root does not have) | **0,0 0x0 (not rendered)** | **64,28 110.51x16 (painted)** |
| `#sel` ("Best") | 123.89,20 28.02x16 | 64,12 110.51x16 |

Controls: the same svg in a block parent is 75.89x22 in RustKit (right), and with `width`/`height` attributes it is 76x22 in a flex parent (right). Reddit's header uses this for the wordmark; `shreddit-sort-dropdown` content ("Open sort options", "Change view") shows because RustKit paints light-DOM children of a custom element whose shadow root does not slot them.

### Extra: H26d, `minmax(0,Xpx)` tracks are distributed proportionally
`reddit_track_distribution_h26d.html` (not a layout cascade problem, found on the way). Container 917px, gap 24, `grid-template-columns: minmax(0, 756px) minmax(0, 316px)`.

| | Chromium 148 | RustKit `f54103d9` |
|---|---|---|
| `#a` / `#b` | **0,0 577x60 / 601,0 316x60** | **0,0 625.38x60 / 649.38,0 267.62x60** |
| pixel (620,40) | 255,204,153 (`#b`) | 153,204,255 (`#a`) |

Chromium grows both tracks equally until `#b` reaches its 316px limit and gives the rest to `#a`; RustKit splits the free space in proportion to the 756:316 limits. The escaped Tailwind class and the `min-width: 960px` media rule are applied correctly (the columns do change).

## Still open on reddit (not reduced here)
- Header: search bar x/width and the garbled Sign Up / Log In / menu boxes (two 40px-tall `rounded_rect` ops 2019px and 1551px wide at x=640 and x=2667).
- Challenge interstitial: `form.requestSubmit()` from a `DOMContentLoaded` script does not navigate under parity-capture.
- Page height: RustKit 2485px vs Chromium 15955px (feed hydration/lazy content) and post action bars (vote/comment/share) not painted in the first posts.
