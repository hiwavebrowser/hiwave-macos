# CSS census fixtures (Atlas order, Census seq 7)

These are acceptance pages for the CSS census gaps, in this order: line-clamp, @supports, :has(), @container, color-mix() in oklab. They are synthetic test fixtures; no engine code is changed.
Each page is 15 lines or fewer and paints 100x100 boxes (or a clamp box) at fixed coordinates: **green means the feature works, red means it doesn't**. Every page's header comment gives its probe pixels.

- RustKit: develop `6f4cce10`, parity-capture `--profile parity`, `--html-file`, 800x600.
- Chrome: Chrome for Testing 148.0.7778.216, headless, 800x600, DPR 1. Chrome reference frames are kept in a private renders repo, not here.

| # | page | feature | expected Chrome result (acceptance) | RustKit develop 6f4cce10 |
|---|---|---|---|---|
| 1 | `css_line_clamp.html` | `display:-webkit-box` + `-webkit-box-orient:vertical` + `-webkit-line-clamp:2` | `#t` rect (10,10,200,50), 2 lines; `#probe` (10,60,200,20); pixel (100,70) = rgb(0,128,0), pixel (100,100) = rgb(255,255,255) | **FAIL**: `#t` is a plain block, 200x150 (6 lines); pixel (100,70) is red (255,0,0) |
| 2 | `css_supports.html` | `@supports` true/false, positive and `not`, incl. `@supports not selector(:popover-open)` | boxes `#a`–`#e` all rgb(0,128,0) at (60,60) (170,60) (280,60) (390,60) (500,60) | **FAIL** 2 of 5: `#b` (`not` of an unsupported value) is red, because `@supports not` is skipped; `#c` (a false positive condition) is red, because `@supports` is applied unconditionally. `#a`, `#d` and `#e` are green, and `#d`/`#e` pass only because the `not` block is skipped |
| 3 | `css_has.html` | `:has(> .x)`, `:not(:has(> .x))`, `:has()` with no match | `#p1`–`#p4` all rgb(0,128,0) at (60,60) (170,60) (280,60) (390,60) | **FAIL** 2 of 4: `#p1` is red (`:has` never matches); `#p2` is red (`:not(:has())` always matches). `#p3` and `#p4` are green by accident of the same rule |
| 4 | `css_container_query.html` | `@container` named and unnamed size queries on `container-type:inline-size` | `#k1`–`#k3` all rgb(0,128,0) at (60,60) (170,60) (280,60) | **FAIL** 2 of 3: `#k1` and `#k2` are red (the true `@container` blocks are skipped). `#k3` is green (its false block is also skipped) |
| 5 | `css_color_mix_oklab.html` | `color-mix(in oklab, …)` direct and through `var()` | (60,60) = rgb(0,128,0); (170,60) and (280,60) = rgb(128,192,128) (computed `oklab(0.519749 -0.140274 0.107678 / 0.5)` over white) | **FAIL** 3 of 3: all three boxes are white (255,255,255), not mixed and not even the red fallback (see note) |

Added later (display re-cut of `243374eb`): `css_display_contents.html`, a `display:contents` wrapper in a flex row (census: 68 of the top sites). Chrome 148.0.7778.216: (60,60) and (160,60) both rgb(0,128,0). RustKit develop `6e25ea3a`: **FAIL**, (160,60) is red (the wrapper kept a block box and stacked its children); green with the re-cut.

Note on 5: in RustKit an unparseable value in the `background` **shorthand** wipes the earlier valid `background: red`, instead of the declaration being dropped. `background:red; background:frobnicate(1)` and `background:red; background:oklab(0.5 0 0)` also paint nothing. The longhand `background-color: red; background-color: color-mix(…)` keeps red. Real sites rely on this fallback pattern (`background: X; background: color-mix(…)`), so the shorthand drop is a separate bug from color-mix() support.

Running a page: `parity-capture --html-file <page> --width 800 --height 600 --dump-frame out.ppm`, then read the probe pixels listed in the page's header comment.
