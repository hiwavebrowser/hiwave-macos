# Reduced repro: external svg `<use href="file.svg#id">` (census tech 8)

- **Finding:** `census-tech-8-external-svg-use`, Census #572 icon technique 8, "`<svg><use href="file.svg#id">` external".
- **Census:** top-80 (`census-top80.md`) 2/57: caranddriver, cloudflare. Top-100 draft (`census-top100.md`, #597/#600) 3/67: caranddriver, cloudflare, target.
- **Extends:** [`../2026-10-06/repro_svg_use.md`](../2026-10-06/repro_svg_use.md) (#576), which covers *local* `<use href="#id">` / `xlink:href` (technique 7). This page covers only the case where `<use>` points at a *separate file*.
- **Page:** [`repro_svg_use_external.html`](repro_svg_use_external.html), 12 lines, plus sprite [`svg_use_sprite.svg`](svg_use_sprite.svg).
- **Status:** written without running Chrome or our engine (quiet hours). Pixels are still to be measured, see the table below.

## How the sites reference the sprite

Sources: `census-top100.json` / `census-top80.json` examples, plus the sites' public HTML and sprite files fetched with plain `curl` on 2026-10-07 (no browser).

| Site | `<use>` count (census) | Reference | Origin | Attribute | Fragment |
|---|:---:|---|---|---|---|
| cloudflare.com | 105 | `/icons.svg#chevronGroup`, `/icons.svg#user2`, … (a single sprite with many `<symbol id=…>`; the target is not the first symbol) | same-origin, root-relative | plain `href` (no `xlink:href`) | yes, a named symbol |
| caranddriver.com | 26 | `/_assets/design-tokens/…/icons/caret-right-regular.svg?embed#icon` (one file per icon, each holding one `<symbol id="icon">`) | same-origin, root-relative, **with a `?embed` query** | plain `href` on 2 uses. Most of them (24 in the static HTML) ship as **`data-href`** and get `href` from script later | yes, always `#icon` |
| target.com (top-100 only) | 15 | `/icons-illustrations/v2/location.svg#location` (one file per icon) | same-origin, root-relative | (not checked) | yes |

Neither site uses `xlink:href` or a cross-origin sprite.

**Census caveat:** `technique_classifier.js` puts every `<use>` whose href doesn't start with `#` in the external bucket, *including an empty href*. All three caranddriver examples in the census have `href: ""` and a 0x0 rect: those are `data-href` uses that hadn't been given an `href` yet when the snapshot ran. caranddriver really does use external sprites (the two plain-`href` arrows and the hydrated icons), but its count of 26 includes uses that had no `href` at all when the census looked.

## The page

The sprite holds two symbols: `#decoy` (solid red `#ff0000`) first and `#icon` (solid `#0066cc`) second, both `viewBox="0 0 20 20"` with one full-size rect. Each case is a 40x40 absolutely positioned `<svg viewBox="0 0 20 20">` (absolute positioning avoids the whitespace issue noted in #576).

- **A (cloudflare):** `<use href="svg_use_sprite.svg#icon"/>`, a plain href to a named symbol that is *not* first in the file.
- **B (caranddriver arrows):** `<use href="svg_use_sprite.svg?embed#icon"/>`, the same file reached through a query string.
- **C (caranddriver, most icons):** `<use data-href="…?embed#icon"/>`, and an inline script copies `data-href` into `href` (the site's hydration step, reduced to one line).

**Measurement note:** Chrome won't load an external `<use>` from a `file://` page (it treats `file:` URLs as unique origins). Serve the directory over http, e.g. `python3 -m http.server` in this folder and load `http://127.0.0.1:<port>/repro_svg_use_external.html`. Do the same for our engine if it resolves external resources over http, or record how `parity-capture --html-file` resolves the sprite. Wait for load/networkidle before you sample, because the sprite is fetched asynchronously.

## Planned probes and expected Chrome result (800x600, DPR 1, pinned Chrome for Testing as in #576)

| Probe | What it tests | Expected Chrome RGBA | Chrome pixel (measured) | RustKit pixel (measured) |
|---|---|---|---|---|
| **(20,20)** | A: external sprite, named symbol | `0,102,204,255` (`#0066cc`) | _TBD post-19:30_ | _TBD post-19:30_ |
| **(20,70)** | B: external sprite + `?embed` query | `0,102,204,255` | _TBD_ | _TBD_ |
| **(20,120)** | C: `data-href` → `href` by script | `0,102,204,255` | _TBD_ | _TBD_ |
| (60,20) | control, outside every icon | `255,255,255,255` | _TBD_ | _TBD_ |

How to read the result: white at a probe means the external `<use>` painted nothing. Red `255,0,0,255` means the fragment resolved to the wrong (first) symbol. Expect our engine to be white everywhere, because #576 found `<use>` unresolved in the renderer even for local references. If so, this page is the follow-up check that `<use>` also fetches the external document after the local fix lands.

## First difference

_To fill in after the post-19:30 measurement._
