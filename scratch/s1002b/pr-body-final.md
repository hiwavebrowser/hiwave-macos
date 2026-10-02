## What

The `background` shorthand never cleared a colour. It set `background-color` only when its whole value was a colour, so every other form left an earlier colour painted: `background: none`, `background: 0 0` (what minifiers write for `none`), an image alone, a gradient alone, `initial`, `unset`. Chrome resets every longhand the shorthand does not name (css-backgrounds-3 §3.10). The reverse was wrong too: a colour beside an image (`background: #fff url(a.png) no-repeat`) was dropped, so the earlier colour stayed where the new one belonged.

This is what kept #429 (the push button's default box and face) in draft: once a button's grey face arrives through the cascade, a page that removes it with `background: none` kept it (weather.com's "More" button).

A fixture on the hub branch (`scratch/s1002b/bgsh.html`): eighteen boxes, each given a colour (or a gradient) by an earlier rule and then one later declaration. The colour painted at each box's centre, pinned Chrome 148 against both arms (`scratch/s1002b/bgsh_cmp.py`):

```
later declaration, over `background-color: red`        Chrome 148   develop 2dd7680   fix
background: none                                       clear        red                clear
background: 0 0                                        clear        red                clear
background: unset                                      clear        red                clear
background: initial                                    clear        red                clear
background: url(nope.png) no-repeat                    clear        red                clear
background: linear-gradient(transparent, transparent)  clear        red                clear
background: green                                      green        green              green
background: green url(nope.png) no-repeat              green        red                green
background: url(nope.png) no-repeat green              green        red                green
background-image: none                                 red          red                red
background: -moz-linear-gradient(top, blue, blue)      red          red                red
background: transparent                                clear        clear              clear
background: inherit (parent is white)                  white        red                red
background: none; background-color: green              green        green              green
background: none !important, then background-color     clear        green              clear
over `background: linear-gradient(blue, blue)`
background: none                                       clear        blue               clear
background: -moz-linear-gradient(top, red, red)        blue         blue               blue

boxes whose colour differs from Chrome                 -            11 of 18           1 of 18
```

The one left is `background: inherit`: the engine treats `inherit` on a property that does not inherit as "leave it", for every such property. Not changed here.

## Change

`crates/rustkit-engine/src/lib.rs`, the `"background" | "background-image"` arm of `apply_style_property` and `apply_initial_value`:

- The shorthand starts from a transparent colour, then takes the colour its value names: the whole value, or one top-level token of a layer (`#fff url(a.png) no-repeat`, either order).
- `background: initial` and `background: unset` clear the colour and the image layers (`apply_initial_value` had no arm for the shorthand).
- The legacy `background_gradient` field is cleared with the layers. Paint falls back to it, so `background: none` over a gradient kept the gradient.
- A value in another engine's prefix (`-moz-`, `-o-`, `-ms-`) is dropped whole, as Chrome drops a declaration it cannot parse. Before, it cleared the earlier image layers; with the reset it would have cleared the colour too.
- `background-image` is a longhand and still leaves the colour alone.

Not in this PR (each measured on the fixtures, none new):

- `background: inherit` (above).
- The shorthand's position, size, repeat and boxes are not read: `background: url(a.png) no-repeat center / cover` keeps only the image, and only when the layer begins with it. That is the next PR, stacked on this one.
- `all: unset` is not applied, and a four-digit hex colour (`#0000`) does not parse.

## Tests

One engine test, `the_background_shorthand_resets_the_colour_it_does_not_name`: the five resets over an earlier colour; a colour alone, before an image and after one; `background-image: none` leaving the colour; a foreign-prefixed value leaving colour and image; `none` over a gradient removing the layer and the legacy gradient.

Fail-first: the test spliced into develop's test module, non-test code untouched (`scratch/s1002b/failfirst_bg.py`):

```
test element_identity_tests::the_background_shorthand_resets_the_colour_it_does_not_name ... FAILED
thread 'element_identity_tests::the_background_shorthand_resets_the_colour_it_does_not_name' (16783466) panicked at crates/rustkit-engine/src/lib.rs:16347:13:
assertion `left == right` failed: `background: none` clears the colour
  left: Color { r: 255, g: 0, b: 0, a: 1.0 }
 right: Color { r: 0, g: 0, b: 0, a: 0.0 }
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 299 filtered out; finished in 6.70s
error: test failed, to rerun pass `-p rustkit-engine --lib`
rc 101
restored: True
```

On the branch: `cargo test -p rustkit-engine --lib the_background_shorthand` 1/1. The whole engine suite was not run locally (the other lanes were building, load 13 to 23, and its GPU guard times out under load), so CI's `unit-suites` is its first full run.

## Receipt

Both arms were release-built from clean sources against the same target directory and run on the develop worktree's fixtures (`scratch/s1001c/camp3.py`): develop `2dd7680`, fix `8f78611`.

```
all:      develop 2026-10-02T03:57:23 26/26 avg 1.1071 | fix 2026-10-02T04:09:00 26/26 avg 1.1069 | 25/26 identical
builtins: develop 2026-10-02T03:53:03  5/5  avg 1.8139 | fix 2026-10-02T04:04:37  5/5  avg 1.8139 | 5/5 identical
micro:    develop 2026-10-02T04:22:31 13/13 avg 0.6548 | fix 2026-10-02T04:03:51 13/13 avg 0.6548 | 13/13 identical
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): the lines that differ between the arms, develop then fix:

```
dev: card-grid: geo_fails=0 paint=0.82911 discrete=0
fix: card-grid: geo_fails=0 paint=0.82914 discrete=0
```

<details><summary>Per-case diff_pct, scope all (develop | fix)</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6742 | 3.6742 |
| article-typography | 4.7857 | 4.7857 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| card-grid | 1.3074 | 1.3034 | moved
| chrome_rustkit | 1.1234 | 1.1234 |
| combinators | 0.6547 | 0.6547 |
| css-selectors | 1.3299 | 1.3299 |
| flex-positioning | 0.6327 | 0.6327 |
| form-controls | 3.2388 | 3.2388 |
| form-elements | 0.9535 | 0.9535 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-backgrounds | 1.0133 | 1.0133 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| image-gallery | 0.5217 | 0.5217 |
| images-intrinsic | 0.3267 | 0.3267 |
| new_tab | 1.1019 | 1.1019 |
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 0.4354 | 0.4354 |
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| specificity | 0.6015 | 0.6015 |
| sticky-scroll | 0.6575 | 0.6575 |

</details>

<details><summary>Per-case diff_pct, scopes builtins and micro (develop | fix)</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6742 | 3.6742 |
| chrome_rustkit | 1.1234 | 1.1234 |
| new_tab | 1.1019 | 1.1019 |
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| combinators | 0.6547 | 0.6547 |
| form-controls | 3.2388 | 3.2388 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| images-intrinsic | 0.3267 | 0.3267 |
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 0.4354 | 0.4354 |
| specificity | 0.6015 | 0.6015 |

</details>

## Real sites

RustKit frames only, each site captured develop, fix, develop, fix at 1280x800 (`scratch/s1001g/ab.py`). "within" is the difference between two captures of the same binary (the site's own variance); "across" is develop against fix, all four pairs.

```
google       within develop   4.09%  within fix   4.09%  across   0.30%   4.39%   4.39%   0.30%  failed: none  secs A,B,A,B 5,5,5,5
youtube      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 7,9,10,8
facebook     within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 16,16,13,15
instagram    within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 17,14,18,16
wikipedia    within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 6,6,5,6
lyft         within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 10,9,11,9
reddit       within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 1,3,1,1
x            within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 5,5,4,4
linkedin     within develop  40.97%  within fix   1.55%  across  40.97%  41.05%   0.00%   1.55%  failed: none  secs A,B,A,B 5,4,4,4
yahoo        within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 10,10,19,22
bing         within develop   0.24%  within fix   0.00%  across   0.24%   0.24%   0.00%   0.00%  failed: none  secs A,B,A,B 7,9,6,7
walmart      within develop  23.34%  within fix   0.00%  across   0.00%   0.00%  23.34%  23.34%  failed: none  secs A,B,A,B 10,14,15,11
microsoft    within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 16,13,13,11
apple        within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 22,22,23,21
netflix      within develop   4.80%  within fix   1.90%  across   4.80%   4.63%   1.95%   2.09%  failed: none  secs A,B,A,B 21,21,24,29
github       within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,30,30,30
shopify      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 29,26,28,27
squarespace  within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,30,31,30
cnn          within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,30,30,30
weather      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 15,23,16,16
```

No site moves between the arms. Seventeen were captured; fourteen are pixel-identical across all four pairs or differ only by what two captures of one binary differ by (google 0.30%, bing 0.24%, netflix 1.9 to 4.8%). linkedin and walmart each served a second variant of the page to one capture (linkedin's first develop capture, walmart's second); the other three captures of each agree to 0.00% and 1.55%. github, squarespace and cnn did not finish within the binary's 30 s on either arm (load 13 to 21 from the other lanes), so they are unverified on both arms alike. No check changes.

I expected this change to move live pages in both directions and it moves none of the first viewports. Where it shows is #429's button face, which is not on develop yet.

The one campaign mover is `card-grid`, 1.3074 -> 1.3034% (99 pixels, paint score 0.82911 -> 0.82914). Its featured card is `.card { background: white }` under a later `.card.featured { background: linear-gradient(...) }`. On develop the white stayed under the gradient and showed as light slivers at the card's four rounded corners; Chrome has no white there, and neither does the fix.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
