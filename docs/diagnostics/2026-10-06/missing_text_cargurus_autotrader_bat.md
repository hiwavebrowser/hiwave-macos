# Missing text on cargurus / autotrader / bringatrailer, sorted by class

- **Finding:** Atlas queue item 2, the "not yet readable" sites from #568's first reading. chrono24 and cars.com are skipped (403).
- **Oracle:** pinned Chrome for Testing **148.0.7778.216** (mac_arm64). Text comes from `tools/parity_oracle/realsite.mjs chrome <url> … 1280 800 3000`, the board's own oracle call.
- **Ours:** `parity-capture --url` at the develop engine (`50e77c83` plus 2 navigation-only commits), 1280x800. The word sets come from `scripts/realsite_board.py`'s own `words()` and `rustkit_viewport_text()`.
- **Pages:** [`cargurus_button_gradient_text_repro.html`](cargurus_button_gradient_text_repro.html) (9 lines) and [`cargurus_event_target_ctor_repro.html`](cargurus_event_target_ctor_repro.html) (11 lines, needs `--url`).

## cargurus.com: most of the "missing" text is painted, but the board doesn't count it

| | Words |
|---|---|
| Chrome first-viewport words | 47 |
| Ours as the board scores it (`op == "text"` runs only) | **0.574** |
| Ours if `button`, `gradient_text` and `text_input` runs also count | **0.936** (passes `READABLE_MIN = 0.80`) |

`rustkit_viewport_text()` (`scripts/realsite_board.py:123`) only reads `op == "text"`. Ours paints two kinds of text through other ops:

- **`button`**: `div._pillsContainer_pyopq_1 > button._pill_pyopq_1` ×5 ("Trucks under $30k", "Great gas mileage", "Hybrids and EVs", "Toyotas", "Great deals under $20k"). They are in the display list as `Button { label: "Trucks under $30k", font_size: 14.0, … }`.
- **`gradient_text`**: `span._askText_199d2_18._askTextRedesign_199d2_32` ("Ask", in `nav#headerNav`) and `div._textContainer_o4jkz_40._textContainer_10rmj_13` ("Search in your own words with AI"). Both use `background-image: linear-gradient(…); background-clip: text; -webkit-text-fill-color: transparent`, and ours emits `GradientText { text: "Ask", … }`.

On the reduced page (`cargurus_button_gradient_text_repro.html`, 800x600) ours paints both glyphs and the board scores ours's text as `''`:

| Probe | Chrome RGBA | Ours RGBA |
|---|---|---|
| **(12,30)** `<button>` label stem | `0,0,0,255` | `0,0,0,255` |
| **(110,30)** gradient-clipped glyph stem | `16,49,120,255` | `69,72,146,255` (painted; the gradient colour at that x differs) |
| `rustkit_viewport_text()` on ours's display list | (Chrome text: "I I") | `''` |

This is a board fix, not an engine one. Atlas/Z-lane own `scripts/`.

What's really missing on cargurus:

- **"Feedback"**: `button._tab_1gfcl_1` (`transform: matrix(0,-1,1,0,0,0)`) < `div._tabPositioner_1gfcl_1` < `div._tabDock_18xm1_1` (`position: fixed; z-index: 1000`) < `astro-island` (`display: contents`). It's absent from ours's live layout but present when Chrome's HAR is replayed into ours, so the server variant decides it.
- **"Search by body style"**: Chrome shows the body-style tab after hydration. Ours shows the server default, "Search by make".
- **Hydration bug found on the way:** the homepage route bundle (`route-EpHdiROt.js:19:27396`, `o0(wE,"instance",new wE)` whose constructor runs `new EventTarget`) throws **`TypeError: Illegal constructor`** in ours. `EventTarget` is created by `iface('EventTarget')` (`crates/rustkit-bindings/src/dom.rs:1096`), and every `iface()` constructor calls `illegal()`. Chrome allows `new EventTarget()`. On the reduced page, served over loopback HTTP because `--html-file` doesn't run scripts, the probe **(50,50)** is Chrome `0,128,0,255` (the listener ran) vs ours `255,0,0,255`. Ours's script log reads `TypeError: Illegal constructor … at <main> (:3:15)`. The second cargurus throw (`RuntimeLimitError: maximum stack size` in `header-footer-html … page.Dgat4dy5.js`, `event:load`) is not reduced.

## autotrader.com: blocked on the Chrome side

From this Mac, pinned Chrome got autotrader's outage page ("We're sorry … site is currently unavailable … incident number …"), so there's no valid oracle text (24 words, all error page). Ours got the real page (`text_input` "Any Make", "Any Model", ZIP, a `button` "Search"). Ours also reports `script_stats: ran 0, over_budget 50`, so every script was over budget. Not reduced; retry the oracle from another network or seat.

## bringatrailer.com: ours fails to render at all

Chrome shows 1,883 characters of first-viewport text. Ours returns `status: error`:

```
Failed to load URL: RenderError("Buffer size 4823762784 bytes exceeds maximum allowed size of 268435456 bytes")
```

4,823,762,784 / (1280 × 4 bytes) ≈ **942,141 px** of document height. Ours makes the page about 940k px tall, the frame buffer is refused, and so no text at all. No layout dump is written on this path, so the oversized box isn't reduced yet. The next step is a layout dump that survives the render error, or a capture with the frame cropped to the viewport, to find the box with a runaway height.
