## What

Pete's "elasticity" check (hub PLAN 2026-09-27 16:20): does a live view re-lay out correctly when it's resized, or does anything stay frozen at the load size?

1. **`parity-capture --resize-to WxH`** (instrument). Loads at `--width x --height`, resizes the live view through `Engine::resize_view` (the same call a window resize makes), and captures at the new size. Compare it with a fresh load at `WxH`.
2. **Scripts see the view's size, and a resize updates it.** `window.innerWidth/innerHeight` were the bindings' 800x600 placeholder on every page, whatever the view's size (a 1280x800 capture reported `innerWidth` 800). `DomBindings::set_dimensions` existed but nothing called it. Now:
   - both binding-creation sites set the view's size;
   - `resize_view` sets the new size and fires `resize` at `window`. Listener exceptions go to the script log as `event:resize`, as the lifecycle events' do.

## What the instrument found (develop 04dd1d1)

Loaded at 1280x800, then resized to 1024x768. Each resized frame was compared with a fresh load at 1024x768, and a second fresh load measured the site's own noise (pixel diff %, any channel > 8/255):

| page | resized vs fresh | fresh vs fresh |
|---|---|---|
| fixture: `@media` max/min-width, vw/vh/dvh, %, flex, `font-size: 3vw` | 0.00 (also 0.00 at 1600x1000) | 0.00 |
| google, wikipedia, x, bing, apple, github, cnn, facebook, yahoo, weather, instagram | 0.00 | 0.00 (google 4.08) |
| linkedin | 1.98 | 1.56 |
| netflix | 1.74 | 2.43 |

**CSS layout does not go stale on resize.** `@media` is re-filtered on every build against the view's current size, and vw/vh stay as units until layout. Every site's resized-vs-fresh diff is 0.00 or within its own fresh-vs-fresh noise. The one stale piece was the JS-visible viewport, which this PR fixes.

Not in this PR: `matchMedia` still answers `matches: false` for every query. Evaluating it needs a native hook into `rustkit_css::media_query_list_matches` from the bindings. That's a follow-up.

## Tests

- `page_script_tests::scripts_see_the_view_size_and_a_resize_updates_it` loads a page into a 200x100 view and expects `200x100`, then resizes to 640x480 and expects `resize:640x480` from a `window` listener. Without the fix, the page reads `800x600` and no `resize` fires. (Seen live on develop: `innerWidth=800` in a 1280x800 capture.)
