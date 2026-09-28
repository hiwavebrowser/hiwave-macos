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

- `cargo test -p rustkit-engine --lib --features headless`: 207 passed, 0 failed.

## Campaign receipt

- `parity_test.py` run `2026-09-27T19:06:48` on a tree byte-identical to head **5d56ddd** (develop 04dd1d1 plus this PR's two files). The develop reference is `2026-09-27T15:52:00`, on #307's head a8e9841, whose fixture code equals develop's.
- **passed 26/26, avg 1.253% (develop 1.253%)**. Identical to develop on all 26 cases. Fixture mode runs with JS off, and this PR changes only the JS bindings path plus a new capture flag.
- Local CI gates (`ratchet_local.py`): "none worse than the committed floor" (exit 2 = tighten-eligible only, no regression). shelf paint 0.96684, the same as develop.
- `wpt_tier1.py` not run (`third_party/wpt` isn't synced in this worktree).
- Real-site board: not re-run for this PR. Scripts run against a stub `document` today (see the hub digest), so no script can change the page yet, and the board can't move. Expected ±0.

<details><summary>Per-case diff_pct (26 cases)</summary>

| case | develop | this PR |
|---|---|---|
| about | 3.75% | 3.75% |
| article-typography | 4.79% | 4.79% |
| backgrounds | 1.22% | 1.22% |
| bg-pure | 0.00% | 0.00% |
| bg-solid | 0.21% | 0.21% |
| card-grid | 1.30% | 1.30% |
| chrome_rustkit | 1.12% | 1.12% |
| combinators | 0.65% | 0.65% |
| css-selectors | 1.50% | 1.50% |
| flex-positioning | 0.68% | 0.68% |
| form-controls | 3.24% | 3.24% |
| form-elements | 0.99% | 0.99% |
| gpu-gradient-regression | 0.39% | 0.39% |
| gradient-backgrounds | 1.01% | 1.01% |
| gradient-no-radius | 0.52% | 0.52% |
| gradient-radius-only | 0.69% | 0.69% |
| gradients | 0.14% | 0.14% |
| image-gallery | 0.52% | 0.52% |
| images-intrinsic | 0.33% | 0.33% |
| new_tab | 1.57% | 1.57% |
| pseudo-classes | 0.43% | 0.43% |
| rounded-corners | 1.32% | 1.32% |
| settings | 2.09% | 2.09% |
| shelf | 2.87% | 2.87% |
| specificity | 0.60% | 0.60% |
| sticky-scroll | 0.66% | 0.66% |

</details>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
