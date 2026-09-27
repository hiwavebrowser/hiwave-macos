## What

LinkedIn's hero (`<img src="https://static.licdn.com/aero-v1/sc/h/<hash>">`) is an SVG served as `image/svg+xml` from a URL with no extension. Two bugs kept it off the page:

1. **0ccf512: route by Content-Type.** `Engine::load_images` sent images to the SVG lane by URL extension only. Extensionless SVG went to the raster decoder and failed with `DecodeError("Unknown image format")`. `ImageManager::fetch_and_decode` now answers an `image/svg+xml` response with `ImageError::Svg(body)`. The raster lane parses that body into `svg_cache`, the same way the extension lane does, and in-flight waiters get the same outcome. The **type decides, never the bytes**: the MIME Sniffing Standard has no SVG image signature, so Chrome shows an SVG served as `application/octet-stream` as a broken image. The test pins that we do too. (This closes the "content-type routing is the named follow-up" note in `load_images`.)
2. **a8e9841: inline `style` paint.** Once the hero loaded, it painted as a black silhouette. All 148 of its shapes set paint with `style="fill: #…"`, and `SvgStyle::parse_attributes` only read presentation attributes. Inline declarations now merge over those attributes (the style attribute wins, SVG 2 §6.8), with `!important` stripped.

Out of scope, and still visible on the hero: `fill-rule: evenodd` is parsed but the renderer ignores `SvgStyle::fill_rule`, so the chair outline fills solid. The flat SVG parser also doesn't nest `<g>`. Both are separate follow-ups.

## Tests (failing-first, both confirmed failing with the fix disabled)

- `rustkit-engine` (headless): `svg_content_type_tests::an_extensionless_img_is_routed_to_the_svg_lane_by_its_content_type`. A local server serves `/h/typed` (`image/svg+xml; charset=utf-8`) and `/h/untyped` (`application/octet-stream`, same bytes). The typed one lands in `svg_cache` at 40×20; the untyped one does not.
- `rustkit-svg`: `test_inline_style_sets_paint_and_beats_presentation_attributes`.
- `rustkit-image`: `only_the_image_svg_xml_essence_is_svg`.

## Real-site board (Chrome 148 headed oracle)

Full run `20260927T2000Z-svgct` at a8e9841 (develop 9f37124 + this PR, **without** #304): **21/60**, loads 13 · readable 6 · looks-right 2, scorable 21/51. The last develop-code run (`20260927T1510Z-automin2`, 7b3187b = develop 566b8fa's code) was 19/60.

| site | develop-code run | this PR | why |
|---|---|---|---|
| **linkedin** | 2 (LOOKS RIGHT 19.0%) | **3 (10.5%, Chrome-vs-Chrome 1.7%)** | this PR: the hero SVG now loads and paints |
| facebook | 1 (15.5%) | 2 (5.8%) | oracle drift: facebook has no image the new lane touches (0 reroutes in its log); Chrome-vs-Chrome 9.9% |

Every other site scored the same as the develop-code run. microsoft now gets 12 SVGs through the new lane but is still blank for unrelated reasons.

## Campaign receipt

`parity_test.py` run `2026-09-27T15:52:00.963084` on a8e9841: **26/26 passed, avg 1.253% (develop 1.253%)**. **Identical to develop on all 26 cases**, builtins included (about, chrome_rustkit, new_tab, settings, shelf). Develop column: run `2026-09-27T13:09:41` at 7b3187b, which is develop 566b8fa's code; develop has had no non-test crate change since (#303 adds tests only, #305 docs).

CI's Gates A and B plus the ratchet, run locally over the 26 captures (`ratchet_local.py`): **output byte-identical to develop's**, ratchet exit 2 (no regression).

`wpt_tier1.py`: not run. This worktree's `third_party/wpt` isn't synced (54/54 manifest paths absent).

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

Note: the raster `ImageManager` still fetches with its own client (no Referer, no shield). That's the open `<img>`-through-ResourceLoader decision, unchanged here.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
