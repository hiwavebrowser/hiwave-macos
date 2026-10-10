# Reduced repro: ebay.com header flyout painted behind later content (#565 extend, H10)

- **Finding:** Atlas H10, "menu behind image" (ebay nav), Pollux #565 Part 3. This trims Pollux's 18-line `menu_behind_image_repro.html` to 12 lines and rebuilds it on the ancestor chain measured on macOS.
- **Source site:** ebay.com. The home page `https://www.ebay.com/` returned eBay's "Error Page" to headless pinned Chrome from this Mac (curl gets 403), so the probe used `https://www.ebay.com/n/all-categories`, which serves the same global header `header#gh`.
- **Oracle:** pinned Chrome for Testing **148.0.7778.216** (mac_arm64), headless, 1280x800 for the live probe (default UA), 800x600 and DPR 1 for the reduced page.
- **Ours:** `parity-capture --html-file` at the develop engine (`50e77c83` plus 2 navigation-only commits).
- **Page:** [`menu_behind_image_repro.html`](menu_behind_image_repro.html), 12 lines.

## What the live page has (pinned Chrome on macOS, after hovering the first flyout target)

The open flyout and its positioned ancestors, innermost first:

| Element | position | z-index | Notes |
|---|---|---|---|
| `div#s0-1-4-6-10[0]-2-dialog.gh-flyout__dialog` | absolute | **9** | `top: 27px; left: -16px`, box 872,35 327x377 once open (`display:none; opacity:0` when closed) |
| `div.gh-flyout.is-active` (in `div.gh-sell-link < div.gh-nav__right-wrap`) | relative | auto | |
| `nav.gh-nav` | relative | **4** | |
| `header#gh.gh-header` | relative | **100000** | 40,8 1200x102 |
| `div.ghw.ghw--loaded` < `div.global-header` < `body` | relative / static | auto | |

`main#mainContent` (static, z auto) is the next sibling of `div.global-header` and starts at y=167, so a flyout running down to y≈412 overlaps it. In Chrome `elementFromPoint` at the dialog's centre column (1035, 224/337/393) lands inside the dialog every time. The header's z-index 100000 stacking context paints above all of `main`.

## Pixels (reduced page, 800x600, DPR 1)

| Probe | What is there in Chrome | Chrome RGBA | Ours RGBA |
|---|---|---|---|
| **(50,100)** dialog over `main` | red dialog | `255,0,0,255` | `0,0,255,255` |
| (50,40) dialog over the header only (control) | red dialog | `255,0,0,255` | `255,0,0,255` |
| (300,100) `main` only (control) | blue | `0,0,255,255` | `0,0,255,255` |

The ours display list is in pure tree order: `#1` header `#eee`, then `#2` dialog red at (20,27) 200x150, then `#3` main blue at (0,50) 1280x200. `#3` overpaints `#2`.

## First difference

Ours paints `main` after the dialog. Chrome paints the dialog as part of the `z-index: 100000` stacking context created by `header { position: relative }`, which goes after every in-flow block. Pollux's root cause is still true on develop `3c1f48ee`:

- `crates/rustkit-layout/src/lib.rs:7075`: `position: relative` is transferred as `Position::Static`.
- `crates/rustkit-layout/src/lib.rs:1611`: `set_z_index` only creates a context when `position != Static`.

Together these mean neither `header` nor `nav` forms a stacking context. The positioned dialog (z 9) is then emitted in the header subtree's normal tree order, ahead of the later in-flow `main`. A fix at either point should turn (50,100) red. Expect the `nav` z-index 4 context and the `.gh-flyout` relative box to matter after that.

This is also the H15 suspect (header `position:relative; z-index:100000` treated as static). See the H15 note in the H13/H15 bundle when it lands.
