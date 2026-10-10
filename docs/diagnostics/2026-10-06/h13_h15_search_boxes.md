# H13 + H15: search boxes on google.com and ebay.com

- **Findings:** Atlas H13 (google.com search input renders larger than Chrome and pushes the logo) and H15 (ebay.com: typing doesn't reach the search box), bundled as Atlas asked.
- **Oracle:** pinned Chrome for Testing **148.0.7778.216** (mac_arm64), headless, 1280x800, DPR 1, default UA.
- **Ours:** `parity-capture` at the develop engine (`50e77c83` plus 2 navigation-only commits). Live loads use `--url`. "Ours elementFromPoint" means the deepest `--dump-layout` box containing the point, because parity-capture has no hit-test dump.
- **Pages:** [`h13_google_logo_block_repro.html`](h13_google_logo_block_repro.html) (14 lines, view at **1280x800**) and [`h15_ebay_search_input_repro.html`](h15_ebay_search_input_repro.html) (10 lines, 800x600).

## H13 google.com

### Like-for-like comparison

Live google.com sends ours a different markup variant than Chrome. Ours gets a left column `div.BnWzKd` that stacks two 48px icons, plus an AI-mode button, so the live box is 688x98. Chrome gets `div.iblpc` and a 584x52 box. To compare the same markup, Chrome's load was recorded with `realsite.mjs record-har` and replayed into ours with `har_server.py` and `--replay-proxy`.

| Box | Chrome | Ours (Chrome's HAR replayed) |
|---|---|---|
| `div.RNNXgb` (search box) | 348,**326** 584x52 | 348,**236** 584x52 |
| `textarea#ti6dpd.gLFyf` | 396,327 447x50 | 396,237 447x50 |
| `div#sI1XGe.acUsEb.lGdWHf.sI1XGe` (logo block) | 0,60 1280x**240** | 0,60 1280x**150** |
| `img#hplogo` (doodle) | 390,**100** 500x200 | 390,**660** 500x200 |

With the same markup the search box is exactly Chrome's size. What differs is the logo block above it: ours makes it 90px shorter, so the box rides up into the logo, and the logo itself lands 560px low.

Authored rules on `div#sI1XGe` and the column it sits in:

```
.plsC5e  { display: flex; flex-direction: column; height: 100%; }      /* parent, 1280x800 */
.acUsEb  { flex-shrink: 0; box-sizing: border-box; }
.lGdWHf  { min-height: 150px; height: calc(100% - 560px); max-height: 290px; }
.sI1XGe  { display: flex; flex-direction: column; align-items: center; }
.naW5gc  { height: 100%; margin-top: auto; }                             /* child holding the logo */
.eI7Peb  { max-height: 230px; position: relative; text-align: center; margin-left: 48px; margin-right: 48px; }
```

### First differing property: `height: calc(100% - 560px)` on `div#sI1XGe`

Chrome resolves it to 800 − 560 = 240px. Ours falls back to `min-height: 150px`. Bisecting `.lGdWHf { height }` on the reduced page in ours:

| `height:` | Ours box height | Chrome |
|---|---|---|
| `calc(100% - 560px)` | 150 | 240 |
| `30%` | 150 | 240 |
| `calc(30%)` | 150 | 240 |
| `240px` / `calc(240px)` | 240 | 240 |
| `30%`, with the parent `.plsC5e` at `height: 800px` | **240** | 240 |
| `30%`, with the parent `display: block; height: 100%` | **240** | 240 |

So `calc` isn't the problem. **A percentage height on a flex item doesn't resolve when its column-flex container's own height is a percentage** (`height: 100%`, through `html` and `body` at 100%). Ours lays the container out at 800px but treats it as indefinite for its items' percentages.

| Probe (reduced page, 1280x800) | Chrome RGBA | Ours RGBA |
|---|---|---|
| **(640,250)** logo (red) in Chrome, search bar (blue) in ours | `255,0,0,255` | `0,0,255,255` |
| (640,320) search bar (blue) in Chrome, empty in ours | `0,0,255,255` | `255,255,255,255` |

Follow-ups seen in the replay but not reduced here:

- Inside `div.naW5gc`, the rule-less first child `div` is 30px tall in Chrome but 600px in ours. That is what pushes `#hplogo` to y=660.
- `div.naW5gc` is 1184px wide in ours where `align-items: center` gives 500px in Chrome.
- The textarea's `padding-top: 14px` isn't applied in ours. A reduced page with Chrome's computed styles gives ours 447x28 against Chrome's 447x42; the box was smaller, not larger.

## H15 ebay.com `#gh-ac`

The home page returned eBay's error page (and later a "Pardon Our Interruption" challenge) to headless Chrome from this Mac, so the probe used `https://www.ebay.com/n/all-categories`, which has the same global header. The authored CSS came from `ir.ebaystatic.com/rs/c/globalheaderweb/index_lcNW.94739856.css`.

| Box | Chrome | Ours |
|---|---|---|
| `input#gh-ac.gh-search-input.gh-tb` | 263,59 **570.2x40**, border-box, `padding: 0 44px`, 16px/24px | 263.5,59.5 **178.8x22.8** |
| `div#gh-ac-wrap.gh-search-input__wrap` (`position: relative; flex: 1; height: 100%`) | 263,59 571.2x40 | 263.5,59.5 179.8x**78.9** |
| `div.gh-search__wrap` (`display: flex; flex: 1; height: 100%; border: 2.5px solid`) | 261,57 735.2x44 | 261,57 **5.0**x88.9 |
| `button.icon-btn.gh-search-input__clear-btn` / `__camera-btn` (`position: absolute; right: .5rem / 8px; top: 50%; transform: translateY(-50%)`) | 793.2,63 32x32 (overlaid at the right) | 402.3,99 32x32 (**in flow, below the input**) |
| elementFromPoint at Chrome's input centre **(548,79)** | `input#gh-ac` | `div#gh-search-box` (the input ends at x=442) |

Chain: `input#gh-ac < div#gh-ac-wrap < div.gh-search__wrap < div#gh-search-box.gh-search-box__wrap < form#gh-f.gh-search < section.gh-header__main < header#gh.gh-header < div.ghw`.

**Atlas's suspicion checked:** `header#gh` is `position: relative; z-index: 100000` in Chrome, and ours does treat it as static (see `menu_behind_image_repro.md`, H10). But no overlay covers the input in either engine. The click misses because **ours lays the input out at its intrinsic size (about 179x23)**, so most of Chrome's input area belongs to the wrapper in ours.

### First difference: form controls ignore `height: 100%` and `position: absolute`

The reduced page uses eBay's rules on a 300x44 wrapper:

| Element | Chrome | Ours | Same CSS on a `<div>` in ours |
|---|---|---|---|
| `input.gh-search-input` with `width: 100%; height: 100%` | 300x44 | 300x**19** | 300x44, padding 44px applied |
| `button.gh-search-input__camera-btn` with `position: absolute; right: 8px; top: 50%` | 260,6 32x32 | **0,19** (in flow) | 260,22 (positioned, then translated) |

| Probe (reduced page, 800x600) | Chrome RGBA | Ours RGBA |
|---|---|---|
| **(130,35)** inside Chrome's input, below ours's 19px input | `255,0,0,255` | `221,221,221,255` (wrapper) |
| (276,22) camera button | `0,0,255,255` | `221,221,221,255` |
| elementFromPoint (150,22), the input centre | `input#gh-ac` | `div#gh-ac-wrap` |

So ours doesn't apply percentage `height` (and doesn't report padding) on `<input>`, and doesn't apply `position: absolute` on `<button>`. The same declarations work on a `<div>`. On the live page the three in-flow icon buttons and the svg make `#gh-ac-wrap` 78.9px tall. The 5px-wide `div.gh-search__wrap` and the 0px-wide `select#gh-cat` are a further flex symptom, not reduced here. Recheck them after the form-control fix.
