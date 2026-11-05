# Chrome board — HiWave's own UI pages, RustKit vs Chromium (CH3.A1)

Run: 2026-11-05 04:07 UTC · develop `bece56a6` · oracle Chromium 143.0.7499.4 · parity-capture `target/release/parity-capture`

Gate (PLAN-z CH0): every row ≤ 0.5% pixel diff. **5/42 rows green**, 37 over the gate, 0 not scored (a capture failed).

Pixel diff % is pixelmatch over the whole viewport (compare_baseline.mjs). Geometry diffs is the count of (box, axis) pairs more than 0.5 px from Chrome's rect on the join selector (layout_oracle_gate.compare_case). Top 3 is the attribution of the diff pixels to elements by selector; where fewer than three elements carry pixels, the worst geometry deltas fill in.

## Rows, worst first

| page | state | size | pixel diff % | geometry diffs | top 3 differing elements | notes |
|---|---|---|---:|---:|---|---|
| chrome | address_focused | 1024x700 | 4.973 | 210 | `#chromeMain` 66.7%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section workspace-section:nth-of-type(3)` 1.9%; `body > div.chrome-main:nth-of-type(4) > div.toolbar:nth-of-type(2)` 1.8% |  |
| chrome | default | 1024x700 | 4.315 | 210 | `#chromeMain` 66.7%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section workspace-section:nth-of-type(3)` 1.9%; `body > div.chrome-main:nth-of-type(4) > div.toolbar:nth-of-type(2)` 1.8% |  |
| chrome | hover_tab2 | 1024x700 | 4.172 | 210 | `#chromeMain` 66.7%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section workspace-section:nth-of-type(3)` 1.9%; `body > div.chrome-main:nth-of-type(4) > div.toolbar:nth-of-type(2)` 1.8% |  |
| chrome | find_open | 1024x700 | 4.149 | 229 | `#chromeMain` 66.7%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section workspace-section:nth-of-type(3)` 1.9%; `body > div.chrome-main:nth-of-type(4) > div.toolbar:nth-of-type(2) > div.address-bar:nth-of-type(3)` 1.4% |  |
| inspector | default | 1024x700 | 3.904 | 50 | `#dom-tree-container` 14.8%; `#styles-container` 4.4%; `body > div.inspector-container > div.inspector-main:nth-of-type(2) > div.panel styles-panel:nth-of-type(3) > div.panel-content:nth-of-type(2) > div:nth-of-type(2) > div.styles-section:nth-of-type(1) > div.box-model:nth-of-type(2)` 3.8% |  |
| chrome | address_focused | 1280x800 | 3.846 | 210 | `#chromeMain` 72.3%; `#tabs` 1.8%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section workspace-section:nth-of-type(3)` 1.6% |  |
| chrome | downloads_open | 1024x700 | 3.599 | 224 | `#chromeMain` 62.5%; `#rightDownloadsList` 11.0%; `body > aside.right-sidebar:nth-of-type(2) > div.right-sidebar-content:nth-of-type(4)` 2.5% |  |
| report | weekly | 1024x700 | 3.485 | 69 | `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.chart-section:nth-of-type(2) > div.chart-container:nth-of-type(2)` 26.9%; `html > body` 10.5%; `body > div.container:nth-of-type(1)` 7.5% |  |
| report | default | 1024x700 | 3.259 | 65 | `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.chart-section:nth-of-type(2) > div.chart-container:nth-of-type(2)` 26.9%; `html > body` 10.5%; `body > div.container:nth-of-type(1)` 7.5% |  |
| inspector | default | 1280x800 | 3.258 | 50 | `#dom-tree-container` 23.4%; `#styles-container` 6.5%; `body > div.inspector-container > div.inspector-toolbar:nth-of-type(1)` 3.1% |  |
| chrome | default | 1280x800 | 3.185 | 210 | `#chromeMain` 72.3%; `#tabs` 1.8%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section workspace-section:nth-of-type(3)` 1.6% |  |
| chrome | sidebar_collapsed | 1024x700 | 3.106 | 237 | `html > body` 81.1%; `#tabs` 2.3%; `body > div.chrome-main:nth-of-type(4) > div.toolbar:nth-of-type(2) > div.address-bar:nth-of-type(3)` 2.1% |  |
| chrome | hover_tab2 | 1280x800 | 3.085 | 210 | `#chromeMain` 72.3%; `#tabs` 1.8%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section workspace-section:nth-of-type(3)` 1.6% |  |
| chrome | find_open | 1280x800 | 3.057 | 229 | `#chromeMain` 72.3%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section workspace-section:nth-of-type(3)` 1.6%; `#tabs` 1.4% |  |
| chrome | address_focused | 1600x1000 | 2.746 | 210 | `#chromeMain` 77.6%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section shelf-section:nth-of-type(4)` 2.3%; `#tabs` 1.9% |  |
| chrome | downloads_open | 1280x800 | 2.561 | 222 | `#chromeMain` 68.8%; `#rightDownloadsList` 9.6%; `body > aside.right-sidebar:nth-of-type(2) > div.right-sidebar-content:nth-of-type(4)` 2.0% |  |
| inspector | default | 1600x1000 | 2.505 | 50 | `#dom-tree-container` 35.7%; `#styles-container` 8.5%; `body > div.inspector-container > div.inspector-toolbar:nth-of-type(1)` 2.7% |  |
| report | weekly | 1280x800 | 2.488 | 69 | `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.chart-section:nth-of-type(2) > div.chart-container:nth-of-type(2)` 33.7%; `html > body` 10%; `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.stats-grid:nth-of-type(1)` 9.9% |  |
| report | default | 1280x800 | 2.332 | 65 | `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.chart-section:nth-of-type(2) > div.chart-container:nth-of-type(2)` 33.7%; `html > body` 10%; `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.stats-grid:nth-of-type(1)` 9.9% |  |
| chrome | sidebar_collapsed | 1280x800 | 2.216 | 235 | `html > body` 84.0%; `#tabs` 2.6%; `body > div.chrome-main:nth-of-type(4) > div.toolbar:nth-of-type(2) > div.address-bar:nth-of-type(3)` 2.0% |  |
| chrome | default | 1600x1000 | 2.163 | 210 | `#chromeMain` 77.6%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section shelf-section:nth-of-type(4)` 2.3%; `#tabs` 1.9% |  |
| chrome | find_open | 1600x1000 | 2.128 | 229 | `#chromeMain` 77.6%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section shelf-section:nth-of-type(4)` 2.3%; `#tabs` 1.4% |  |
| chrome | hover_tab2 | 1600x1000 | 2.099 | 210 | `#chromeMain` 77.6%; `body > aside.sidebar:nth-of-type(1) > section.sidebar-section shelf-section:nth-of-type(4)` 2.3%; `#tabs` 1.9% |  |
| settings | default | 1024x700 | 2.090 | 256 | `html > body` 34.8%; `body > div.container` 6.8%; `body > div.container > div.section:nth-of-type(1)` 6.6% |  |
| chrome | downloads_open | 1600x1000 | 1.739 | 225 | `#chromeMain` 75.0%; `#rightDownloadsList` 8.6%; `#tabs` 1.9% |  |
| report | weekly | 1600x1000 | 1.698 | 69 | `html > body` 27.4%; `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.chart-section:nth-of-type(2) > div.chart-container:nth-of-type(2)` 21.6%; `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.chart-section:nth-of-type(3) > div.chart-container:nth-of-type(2)` 8.1% |  |
| settings | default | 1280x800 | 1.679 | 256 | `html > body` 47.5%; `body > div.container` 4.8%; `body > div.container > div.section:nth-of-type(1)` 4.6% |  |
| report | default | 1600x1000 | 1.596 | 65 | `html > body` 27.4%; `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.chart-section:nth-of-type(2) > div.chart-container:nth-of-type(2)` 21.6%; `body > div.container:nth-of-type(1) > div:nth-of-type(5) > div.chart-section:nth-of-type(3) > div.chart-container:nth-of-type(2)` 8.1% |  |
| chrome | sidebar_collapsed | 1600x1000 | 1.518 | 238 | `html > body` 87.5%; `#tabs` 2.4%; `body > div.chrome-main:nth-of-type(4) > div.toolbar:nth-of-type(2) > div.address-bar:nth-of-type(3)` 1.7% |  |
| settings | default | 1600x1000 | 1.451 | 256 | `html > body` 57.6%; `body > div.container` 3.8%; `body > div.container > div.section:nth-of-type(1)` 3.0% |  |
| inspector | console_tab | 1024x700 | 0.873 | 6 | `#console-output` 70.4%; `body > div.inspector-container > div.inspector-main:nth-of-type(2) > div.console-panel active:nth-of-type(4) > div.console-toolbar:nth-of-type(1)` 4.0%; `body > div.inspector-container > div.inspector-toolbar:nth-of-type(1)` 3.5% |  |
| shelf | shelf_search | 1024x700 | 0.850 | 0 | `#commandResults` 77.2%; `body > div.command-palette:nth-of-type(2)` 7.9%; `body > div.shelf-header:nth-of-type(1)` 5.6% |  |
| chrome | focus_mode | 1024x700 | 0.718 | 237 | `html > body.focus-mode` 81.1%; `body > div.chrome-main:nth-of-type(4) > div.toolbar:nth-of-type(2) > div.address-bar:nth-of-type(3)` 2.1%; `#tabs` 2.1% |  |
| shelf | shelf_search | 1280x800 | 0.695 | 0 | `#commandResults` 81.1%; `body > div.command-palette:nth-of-type(2)` 6.8%; `body > div.shelf-header:nth-of-type(1)` 4.9% |  |
| inspector | console_tab | 1280x800 | 0.611 | 6 | `#console-output` 74.1%; `body > div.inspector-container > div.inspector-main:nth-of-type(2) > div.console-panel active:nth-of-type(4) > div.console-toolbar:nth-of-type(1)` 3.6%; `body > div.inspector-container > div.inspector-toolbar:nth-of-type(1)` 3.3% |  |
| shelf | shelf_search | 1600x1000 | 0.526 | 0 | `#commandResults` 85.0%; `body > div.command-palette:nth-of-type(2)` 5.4%; `body > div.shelf-header:nth-of-type(1)` 4.0% |  |
| chrome | focus_mode | 1280x800 | 0.502 | 235 | `html > body.focus-mode` 84.0%; `#tabs` 2.3%; `body > div.chrome-main:nth-of-type(4) > div.toolbar:nth-of-type(2) > div.address-bar:nth-of-type(3)` 2.0% |  |
| shelf | default | 1024x700 | 0.467 | 0 | `#commandResults` 74.1%; `body > div.command-palette:nth-of-type(2)` 7.9%; `body > div.shelf-header:nth-of-type(1)` 5.6% |  |
| inspector | console_tab | 1600x1000 | 0.391 | 6 | `#console-output` 79.3%; `body > div.inspector-container > div.inspector-main:nth-of-type(2) > div.console-panel active:nth-of-type(4) > div.console-toolbar:nth-of-type(1)` 3.0%; `body > div.inspector-container > div.inspector-toolbar:nth-of-type(1)` 2.8% |  |
| shelf | default | 1280x800 | 0.327 | 0 | `#commandResults` 79.3%; `body > div.command-palette:nth-of-type(2)` 6.8%; `body > div.shelf-header:nth-of-type(1)` 4.9% |  |
| chrome | focus_mode | 1600x1000 | 0.322 | 238 | `html > body.focus-mode` 87.5%; `#tabs` 2.1%; `body > div.chrome-main:nth-of-type(4) > div.toolbar:nth-of-type(2) > div.address-bar:nth-of-type(3)` 1.7% |  |
| shelf | default | 1600x1000 | 0.211 | 0 | `#commandResults` 83.8%; `body > div.command-palette:nth-of-type(2)` 5.4%; `body > div.shelf-header:nth-of-type(1)` 4.0% |  |

## Pages

| page | rows | green | worst state | worst pixel diff % |
|---|---:|---:|---|---:|
| chrome | 21 | 1 | address_focused 1024x700 | 4.973 |
| inspector | 6 | 1 | default 1024x700 | 3.904 |
| report | 6 | 0 | weekly 1024x700 | 3.485 |
| settings | 3 | 0 | default 1024x700 | 2.090 |
| shelf | 6 | 3 | shelf_search 1024x700 | 0.850 |

## CSS census red list per page

Census 2026-11-05 (`target/chrome_board/census/CSS_PROPERTY_CENSUS_2026-11-05.json`, oracle 143.0.7499.4): 200 red properties. Per page, the red properties the page declares (in its `<style>` blocks and `style=` attributes).

| page | declared properties | red | red properties |
|---|---:|---:|---|
| chrome | 93 | 33 | `-webkit-text-fill-color` (computed), `-webkit-user-select` (unsupported), `animation` (parse), `animation-delay` (parse), `animation-duration` (parse), `animation-fill-mode` (parse), `animation-iteration-count` (parse), `appearance` (unsupported), `backdrop-filter` (parse), `background-clip` (computed), `background-image` (computed), `background-position` (computed), `background-repeat` (computed), `background-size` (computed), `border-style` (parse), `box-shadow` (computed), `cursor` (unsupported), `display` (computed), `filter` (computed), `flex-direction` (computed), `font-family` (computed), `letter-spacing` (computed), `overscroll-behavior` (unsupported), `scrollbar-color` (unsupported), `scrollbar-width` (unsupported), `text-align` (computed), `text-decoration` (parse), `text-transform` (computed), `transform-origin` (computed), `transition` (parse), `transition-duration` (parse), `user-select` (unsupported), `z-index` (computed) |
| shelf | 50 | 12 | `animation` (parse), `box-shadow` (computed), `cursor` (unsupported), `display` (computed), `flex-direction` (computed), `font-family` (computed), `letter-spacing` (computed), `text-align` (computed), `text-transform` (computed), `transition` (parse), `user-select` (unsupported), `z-index` (computed) |
| inspector | 56 | 11 | `cursor` (unsupported), `display` (computed), `flex-direction` (computed), `font-family` (computed), `letter-spacing` (computed), `text-align` (computed), `text-decoration` (parse), `text-transform` (computed), `transition` (parse), `user-select` (unsupported), `vertical-align` (parse) |
| settings | 60 | 18 | `-webkit-text-fill-color` (computed), `accent-color` (unsupported), `animation` (parse), `animation-delay` (parse), `animation-duration` (parse), `background-clip` (computed), `box-shadow` (computed), `cursor` (unsupported), `display` (computed), `flex-direction` (computed), `font-family` (computed), `letter-spacing` (computed), `scrollbar-color` (unsupported), `scrollbar-width` (unsupported), `text-align` (computed), `text-transform` (computed), `transition` (parse), `transition-duration` (parse) |
| report | 48 | 14 | `-webkit-text-fill-color` (computed), `background-clip` (computed), `box-shadow` (computed), `cursor` (unsupported), `display` (computed), `flex-direction` (computed), `font-family` (computed), `letter-spacing` (computed), `scrollbar-color` (unsupported), `scrollbar-width` (unsupported), `text-align` (computed), `text-transform` (computed), `transition` (parse), `z-index` (computed) |

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
