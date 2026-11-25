# Chrome board — HiWave's own UI pages, RustKit vs Chromium (CH3.A1)

Run: 2026-11-24 17:43 UTC · head `19614aa5a` on develop `e675eeedc` · oracle Chrome for Testing 148.0.7778.216 (PARITY_CHROME_PATH) · parity-capture `batch 126 candidate (parity profile)`

Oracle history: metrics start on 148 from 2026-11-17; earlier rows were measured against Chromium 143 (Playwright-bundled 143.0.7499.4, PARITY_CHROME_PATH unset), so a row's change across that date may be the oracle, not RustKit.

Gate (PLAN-z CH0, docs/GATES.md): a row is green at ≤ 0.5% pixel diff, OR at 0 geometry diffs and ≤ 1.5% pixel diff (geometry matching Chrome leaves only rasterisation; a row whose geometry compare did not run or whose join failed has only the first rule). **42/42 rows green**, 0 over the gate, 0 not scored (a capture failed).

Pixel diff % is pixelmatch over the whole viewport (compare_baseline.mjs). Geometry diffs is the count of (box, axis) pairs more than 0.5 px from Chrome's rect on the join selector (layout_oracle_gate.compare_case). Top 3 is the attribution of the diff pixels to elements by selector; where fewer than three elements carry pixels, the worst geometry deltas fill in. Gate is the rule that made the row green (`pixel` or `geometry`), or `red`.

## Rows, worst first

| page | state | size | pixel diff % | geometry diffs | gate | top 3 differing elements | notes |
|---|---|---|---:|---:|---|---|---|
| chrome | address_focused | 1024x700 | 0.582 | 0 | geometry | `#chromeMain` 66.6%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section workspace-section:nth-of-type(3)` 1.9%; `body > div.chrome-main:nth-of-type(5) > div.toolbar:nth-of-type(2)` 1.8% |  |
| report | weekly | 1024x700 | 0.526 | 0 | geometry | `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.chart-section:nth-of-type(2) > div.chart-container:nth-of-type(2)` 26.9%; `html > body` 10.5%; `body > div.container:nth-of-type(1)` 7.5% |  |
| chrome | address_focused | 1280x800 | 0.508 | 0 | geometry | `#chromeMain` 72.1%; `#tabs` 1.8%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section workspace-section:nth-of-type(3)` 1.6% |  |
| report | weekly | 1280x800 | 0.486 | 0 | pixel | `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.chart-section:nth-of-type(2) > div.chart-container:nth-of-type(2)` 33.7%; `html > body` 10%; `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.stats-grid:nth-of-type(1)` 9.9% |  |
| chrome | address_focused | 1600x1000 | 0.407 | 0 | pixel | `#chromeMain` 77.6%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section shelf-section:nth-of-type(4)` 2.3%; `#tabs` 1.9% |  |
| chrome | default | 1024x700 | 0.403 | 0 | pixel | `#chromeMain` 66.6%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section workspace-section:nth-of-type(3)` 1.9%; `body > div.chrome-main:nth-of-type(5) > div.toolbar:nth-of-type(2)` 1.8% |  |
| chrome | hover_tab2 | 1024x700 | 0.403 | 0 | pixel | `#chromeMain` 66.6%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section workspace-section:nth-of-type(3)` 1.9%; `body > div.chrome-main:nth-of-type(5) > div.toolbar:nth-of-type(2)` 1.8% |  |
| report | weekly | 1600x1000 | 0.397 | 0 | pixel | `html > body` 27.4%; `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.chart-section:nth-of-type(2) > div.chart-container:nth-of-type(2)` 21.6%; `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.chart-section:nth-of-type(3) > div.chart-container:nth-of-type(2)` 8.1% |  |
| chrome | find_open | 1024x700 | 0.322 | 0 | pixel | `#chromeMain` 66.6%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section workspace-section:nth-of-type(3)` 1.9%; `body > div.chrome-main:nth-of-type(5) > div.toolbar:nth-of-type(2) > div.address-bar:nth-of-type(3)` 1.4% |  |
| inspector | default | 1024x700 | 0.322 | 3 | pixel | `#dom-tree-container` 14.8%; `#styles-container` 4.4%; `body > div.inspector-container > div.inspector-main:nth-of-type(2) > div.panel styles-panel:nth-of-type(3) > div.panel-content:nth-of-type(2) > div:nth-of-type(2) > div.styles-section:nth-of-type(1) > div.box-model:nth-of-type(2)` 3.8% |  |
| report | default | 1024x700 | 0.284 | 0 | pixel | `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.chart-section:nth-of-type(2) > div.chart-container:nth-of-type(2)` 26.9%; `html > body` 10.5%; `body > div.container:nth-of-type(1)` 7.5% |  |
| chrome | default | 1280x800 | 0.283 | 0 | pixel | `#chromeMain` 72.1%; `#tabs` 1.8%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section workspace-section:nth-of-type(3)` 1.6% |  |
| chrome | hover_tab2 | 1280x800 | 0.283 | 0 | pixel | `#chromeMain` 72.1%; `#tabs` 1.8%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section workspace-section:nth-of-type(3)` 1.6% |  |
| report | default | 1600x1000 | 0.279 | 0 | pixel | `html > body` 27.4%; `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.chart-section:nth-of-type(2) > div.chart-container:nth-of-type(2)` 21.6%; `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.chart-section:nth-of-type(3) > div.chart-container:nth-of-type(2)` 8.1% |  |
| chrome | downloads_open | 1024x700 | 0.266 | 0 | pixel | `#chromeMain` 62.3%; `#rightDownloadsList` 11.0%; `body > aside.right-sidebar:nth-of-type(2) > div.right-sidebar-content:nth-of-type(4)` 2.5% |  |
| chrome | sidebar_collapsed | 1024x700 | 0.248 | 0 | pixel | `html > body` 81.0%; `#tabs` 2.3%; `body > div.chrome-main:nth-of-type(5) > div.toolbar:nth-of-type(2) > div.address-bar:nth-of-type(3)` 2.1% |  |
| chrome | find_open | 1280x800 | 0.230 | 0 | pixel | `#chromeMain` 72.1%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section workspace-section:nth-of-type(3)` 1.6%; `body > div.chrome-main:nth-of-type(5) > div.toolbar:nth-of-type(2) > div.address-bar:nth-of-type(3)` 1.4% |  |
| inspector | default | 1280x800 | 0.225 | 3 | pixel | `#dom-tree-container` 23.4%; `#styles-container` 6.5%; `body > div.inspector-container > div.inspector-toolbar:nth-of-type(1)` 3.1% |  |
| report | default | 1280x800 | 0.224 | 0 | pixel | `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.chart-section:nth-of-type(2) > div.chart-container:nth-of-type(2)` 33.7%; `html > body` 10%; `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.stats-grid:nth-of-type(1)` 9.9% |  |
| settings | default | 1024x700 | 0.215 | 0 | pixel | `html > body` 34.8%; `body > div.container` 6.8%; `body > div.container > div.section:nth-of-type(1)` 6.6% |  |
| inspector | console_tab | 1024x700 | 0.191 | 0 | pixel | `#console-output` 70.4%; `body > div.inspector-container > div.inspector-main:nth-of-type(2) > div.console-panel active:nth-of-type(4) > div.console-toolbar:nth-of-type(1)` 4.0%; `body > div.inspector-container > div.inspector-toolbar:nth-of-type(1)` 3.5% |  |
| chrome | downloads_open | 1280x800 | 0.186 | 0 | pixel | `#chromeMain` 68.7%; `#rightDownloadsList` 9.6%; `body > aside.right-sidebar:nth-of-type(2) > div.right-sidebar-content:nth-of-type(4)` 2.0% |  |
| chrome | default | 1600x1000 | 0.183 | 0 | pixel | `#chromeMain` 77.6%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section shelf-section:nth-of-type(4)` 2.3%; `#tabs` 1.9% |  |
| chrome | hover_tab2 | 1600x1000 | 0.183 | 0 | pixel | `#chromeMain` 77.6%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section shelf-section:nth-of-type(4)` 2.3%; `#tabs` 1.9% |  |
| settings | default | 1280x800 | 0.176 | 0 | pixel | `html > body` 47.5%; `body > div.container` 4.8%; `body > div.container > div.section:nth-of-type(1)` 4.6% |  |
| chrome | sidebar_collapsed | 1280x800 | 0.173 | 0 | pixel | `html > body` 83.9%; `#tabs` 2.6%; `body > div.chrome-main:nth-of-type(5) > div.toolbar:nth-of-type(2) > div.address-bar:nth-of-type(3)` 2.0% |  |
| shelf | default | 1024x700 | 0.161 | 0 | pixel | `#commandResults` 74.1%; `body > div.command-palette:nth-of-type(2)` 7.9%; `body > div.shelf-header:nth-of-type(1)` 5.6% |  |
| settings | default | 1600x1000 | 0.157 | 0 | pixel | `html > body` 57.6%; `body > div.container` 3.8%; `body > div.container > div.section:nth-of-type(1)` 3.0% |  |
| chrome | find_open | 1600x1000 | 0.150 | 0 | pixel | `#chromeMain` 77.6%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section shelf-section:nth-of-type(4)` 2.3%; `#tabs` 1.3% |  |
| inspector | default | 1600x1000 | 0.144 | 3 | pixel | `#dom-tree-container` 35.7%; `#styles-container` 8.5%; `body > div.inspector-container > div.inspector-toolbar:nth-of-type(1)` 2.7% |  |
| inspector | console_tab | 1280x800 | 0.134 | 0 | pixel | `#console-output` 74.1%; `body > div.inspector-container > div.inspector-main:nth-of-type(2) > div.console-panel active:nth-of-type(4) > div.console-toolbar:nth-of-type(1)` 3.6%; `body > div.inspector-container > div.inspector-toolbar:nth-of-type(1)` 3.3% |  |
| chrome | downloads_open | 1600x1000 | 0.119 | 0 | pixel | `#chromeMain` 74.9%; `#rightDownloadsList` 8.6%; `#tabs` 1.9% |  |
| shelf | default | 1280x800 | 0.114 | 0 | pixel | `#commandResults` 79.3%; `body > div.command-palette:nth-of-type(2)` 6.8%; `body > div.shelf-header:nth-of-type(1)` 4.9% |  |
| chrome | sidebar_collapsed | 1600x1000 | 0.111 | 0 | pixel | `html > body` 87.4%; `#tabs` 2.4%; `body > div.chrome-main:nth-of-type(5) > div.toolbar:nth-of-type(2) > div.address-bar:nth-of-type(3)` 1.7% |  |
| inspector | console_tab | 1600x1000 | 0.086 | 0 | pixel | `#console-output` 79.3%; `body > div.inspector-container > div.inspector-main:nth-of-type(2) > div.console-panel active:nth-of-type(4) > div.console-toolbar:nth-of-type(1)` 3.0%; `body > div.inspector-container > div.inspector-toolbar:nth-of-type(1)` 2.8% |  |
| shelf | default | 1600x1000 | 0.071 | 0 | pixel | `#commandResults` 83.8%; `body > div.command-palette:nth-of-type(2)` 5.4%; `body > div.shelf-header:nth-of-type(1)` 4.0% |  |
| chrome | focus_mode | 1024x700 | 0.053 | 0 | pixel | `html > body.focus-mode` 81.0%; `body > div.chrome-main:nth-of-type(5) > div.toolbar:nth-of-type(2) > div.address-bar:nth-of-type(3)` 2.1%; `#tabs` 2.1% |  |
| shelf | shelf_search | 1024x700 | 0.050 | 0 | pixel | `#commandResults` 77.2%; `body > div.command-palette:nth-of-type(2)` 7.9%; `body > div.shelf-header:nth-of-type(1)` 5.6% |  |
| chrome | focus_mode | 1280x800 | 0.037 | 0 | pixel | `html > body.focus-mode` 83.9%; `#tabs` 2.3%; `body > div.chrome-main:nth-of-type(5) > div.toolbar:nth-of-type(2) > div.address-bar:nth-of-type(3)` 2.0% |  |
| shelf | shelf_search | 1280x800 | 0.035 | 0 | pixel | `#commandResults` 81.1%; `body > div.command-palette:nth-of-type(2)` 6.8%; `body > div.shelf-header:nth-of-type(1)` 4.9% |  |
| chrome | focus_mode | 1600x1000 | 0.024 | 0 | pixel | `html > body.focus-mode` 87.4%; `#tabs` 2.1%; `body > div.chrome-main:nth-of-type(5) > div.toolbar:nth-of-type(2) > div.address-bar:nth-of-type(3)` 1.7% |  |
| shelf | shelf_search | 1600x1000 | 0.024 | 0 | pixel | `#commandResults` 85.0%; `body > div.command-palette:nth-of-type(2)` 5.4%; `body > div.shelf-header:nth-of-type(1)` 4.0% |  |

## Pages

| page | rows | green | worst state | worst pixel diff % |
|---|---:|---:|---|---:|
| chrome | 21 | 21 | address_focused 1024x700 | 0.582 |
| inspector | 6 | 6 | default 1024x700 | 0.322 |
| report | 6 | 6 | weekly 1024x700 | 0.526 |
| settings | 3 | 3 | default 1024x700 | 0.215 |
| shelf | 6 | 6 | default 1024x700 | 0.161 |

## CSS census red list per page

Census: the 2026-11-03 Chromium 143 census (`CSS_PROPERTY_CENSUS_2026-11-03.json`, oracle 143.0.7499.4; NOT this board's oracle (148.0.7778.216): the census has not been retaken on it): 212 red properties. Per page, the red properties the page declares (in its `<style>` blocks and `style=` attributes).

| page | declared properties | red | red properties |
|---|---:|---:|---|
| chrome | 93 | 36 | `-webkit-text-fill-color` (parse), `-webkit-user-select` (unsupported), `animation` (parse), `animation-delay` (parse), `animation-duration` (parse), `animation-fill-mode` (parse), `animation-iteration-count` (parse), `appearance` (unsupported), `backdrop-filter` (unsupported), `background-clip` (parse), `background-image` (parse), `background-position` (parse), `background-repeat` (parse), `background-size` (parse), `border-style` (parse), `box-shadow` (parse), `cursor` (unsupported), `display` (computed), `filter` (parse), `flex-direction` (computed), `font-family` (computed), `letter-spacing` (computed), `outline` (unsupported), `outline-color` (unsupported), `outline-offset` (unsupported), `overscroll-behavior` (unsupported), `scrollbar-color` (unsupported), `scrollbar-width` (unsupported), `text-align` (computed), `text-decoration` (parse), `text-transform` (computed), `transform-origin` (parse), `transition` (parse), `transition-duration` (parse), `user-select` (unsupported), `z-index` (computed) |
| shelf | 50 | 13 | `animation` (parse), `box-shadow` (parse), `cursor` (unsupported), `display` (computed), `flex-direction` (computed), `font-family` (computed), `letter-spacing` (computed), `outline` (unsupported), `text-align` (computed), `text-transform` (computed), `transition` (parse), `user-select` (unsupported), `z-index` (computed) |
| inspector | 56 | 13 | `cursor` (unsupported), `display` (computed), `flex-direction` (computed), `font-family` (computed), `letter-spacing` (computed), `outline` (unsupported), `outline-offset` (unsupported), `text-align` (computed), `text-decoration` (parse), `text-transform` (computed), `transition` (parse), `user-select` (unsupported), `vertical-align` (parse) |
| settings | 60 | 19 | `-webkit-text-fill-color` (parse), `accent-color` (unsupported), `animation` (parse), `animation-delay` (parse), `animation-duration` (parse), `background-clip` (parse), `box-shadow` (parse), `cursor` (unsupported), `display` (computed), `flex-direction` (computed), `font-family` (computed), `letter-spacing` (computed), `outline` (unsupported), `scrollbar-color` (unsupported), `scrollbar-width` (unsupported), `text-align` (computed), `text-transform` (computed), `transition` (parse), `transition-duration` (parse) |
| report | 48 | 14 | `-webkit-text-fill-color` (parse), `background-clip` (parse), `box-shadow` (parse), `cursor` (unsupported), `display` (computed), `flex-direction` (computed), `font-family` (computed), `letter-spacing` (computed), `scrollbar-color` (unsupported), `scrollbar-width` (unsupported), `text-align` (computed), `text-transform` (computed), `transition` (parse), `z-index` (computed) |

## States

Actions run identically by parity-capture `--actions` and chrome_board.mjs:

- `address_focused`: `[{"type": "click", "selector": "#urlInput"}, {"type": "key", "selector": "#urlInput", "text": "hiwave.dev/docs"}]`
- `console_tab`: `[{"type": "click", "selector": ".toolbar-tab[data-tab=\"console\"]"}]`
- `default`: `[]`
- `downloads_open`: `[{"type": "click", "selector": "#toolbarMenuBtn"}]`
- `find_open`: `[{"type": "key", "key": "F3"}]`
- `focus_mode`: `[{"type": "key", "key": "F", "modifiers": ["Control", "Shift"]}]`
- `hover_tab2`: `[{"type": "hover", "selector": "#tabs .tab:nth-child(2)"}]`
- `shelf_search`: `[{"type": "click", "selector": "#commandInput"}, {"type": "key", "selector": "#commandInput", "text": "sh"}]`
- `sidebar_collapsed`: `[{"type": "click", "selector": "#sidebarLogoToggle"}]`
- `weekly`: `[{"type": "click", "selector": ".period-btn[data-period=\"weekly\"]"}]`
