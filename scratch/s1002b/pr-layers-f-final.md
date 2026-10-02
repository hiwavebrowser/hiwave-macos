Follows #433 (the shorthand's colour reset), which merged while this was being measured. The base arm of the receipts is #433's head `8f78611`; develop `33d7368` is its merge commit and has the same tree (`git diff 8f78611 33d7368` is empty), so that arm is develop.

## What

A layer of the `background` shorthand kept only its image, and only when the layer began with it. Everything else the layer says was dropped:

- `background: url(a.png) no-repeat center / cover` tiled from the top left at the image's own size.
- `background: no-repeat center url(a.png)` had no image at all.
- `background: linear-gradient(...) no-repeat 0 100% / 100% 2px` (an underline, a progress bar, a divider) painted nothing: the gradient parser was handed the whole layer and refused it.
- A `url(...)` ended at the first `)`, which a `data:` URL may contain.

`background-position` also read its keywords by place instead of by axis: `bottom` alone meant "right edge", and `top right` put `top` on the horizontal axis.

A fixture on the hub branch (`scratch/s1002b/bgsh3.html`, twenty 120 x 60 boxes; the image is a 20 x 20 `data:` PNG with four coloured quadrants, and gradients). Share of each box's pixels that differ from pinned Chrome 148 (`scratch/s1002b/box_cmp.py`):

```
declaration (IMG = the data: PNG, GRAD = linear-gradient(blue, blue))                                 before #433  develop 33d7368  fix
background-image: IMG; background-repeat: no-repeat; background-position: center                            0.00%            0.00%    0.00%
background: IMG no-repeat                                                                                  94.44%           94.44%    0.00%
background: IMG no-repeat center                                                                          100.00%          100.00%    0.00%
background: IMG no-repeat right bottom                                                                     94.44%           94.44%    0.00%
background: IMG repeat-x                                                                                   66.67%           66.67%    0.00%
background: IMG no-repeat 10px 20px / 40px 30px                                                            96.50%           96.50%   29.17%
background: #ff0 IMG no-repeat center                                                                     100.00%            5.56%    0.00%
background: GRAD no-repeat 20px 10px / 30px 15px                                                            6.25%            6.25%    0.00%
background: IMG center / cover no-repeat                                                                   71.67%           71.67%    0.00%
background: IMG center / contain no-repeat #eee                                                            89.44%           89.44%    0.00%
background: IMG                                                                                             0.00%            0.00%    0.00%
background: no-repeat center IMG                                                                            5.56%            5.56%    0.00%
background: IMG 50% 50% no-repeat, linear-gradient(#cfc, #cfc)                                            100.00%          100.00%    0.00%
background: GRAD no-repeat bottom / 100% 4px                                                                6.67%            6.67%    0.00%
background: GRAD repeat-y right / 6px 12px                                                                  5.00%            5.00%    0.00%
background: GRAD no-repeat top right / 30px 15px                                                            6.25%            6.25%    0.00%
background: GRAD no-repeat left 10px top 20px / 30px 15px                                                   6.25%            6.25%    0.00%
background: GRAD no-repeat; background-size: 50% 50%; background-position: bottom                          25.00%           25.00%    0.00%
background: #cfc GRAD no-repeat center/50% 50%                                                            100.00%           25.00%    0.00%
background: GRAD 0 0/20px 100% no-repeat, linear-gradient(red, red) 100% 0/20px 100% no-repeat, #eee       33.33%           33.33%    0.00%

boxes more than 1% off Chrome                                                                            18 of 20         18 of 20  1 of 20
```

The one left is a url image at a length position (`10px 20px`): the display list's image command carries the position as two fractions, so a length is dropped and the image sits at the corner. Gradients take the length (rows 8 and 17). It is the same on develop for the longhand, and it goes with the fetch PR, where sprites need it.

## Change

`crates/rustkit-engine/src/lib.rs`:

- `parse_background_shorthand_layer` reads one layer token by token (css-backgrounds-3 §3.10): the image wherever it stands (`url()` to its closing parenthesis, or a gradient), the position with an optional `/ size`, one or two repeat keywords (`repeat no-repeat` is `repeat-x`), the attachment (read, not used), one or two boxes (the first sets origin and clip, the second the clip), and the colour. The `"background"` arm of `apply_style_property` uses it; `background-image` still takes the image alone.
- `parse_background_position`: one value centres the other axis, and `top` / `bottom` are vertical; two keywords are read by axis in either order; the three- and four-value form (`left 10px top 20px`) keeps an offset from `left` or `top`.

Not in this PR:

- An offset from the far edge (`right 5px bottom 5px`): the position value cannot say "from the far edge" yet, so the image sits on that edge.
- A token the layer parser does not know (`image-set()`, a `calc()` length, `em` units) is skipped; Chrome would use it or drop the declaration.
- **A CSS background image is never fetched.** Only `<img>` elements are discovered (`Engine::discover_images`); a `background-image: url(...)` reaches the display list and is painted only if it is a `data:` URL. This PR is the part that has to land first: once images are fetched, a `no-repeat` sprite that tiles would be worse than no image. The fetch is the next PR.
- A longhand that arrives before its layer exists (`background-size: cover` in one rule, `background-image` in a later one) is still lost, as on develop.

## Tests

Three engine tests:

- `the_background_shorthand_sets_each_layers_position_size_and_repeat`: image first and image last, with and without spaces around the slash, a colour beside a gradient with lengths, two layers each keeping its own, the initial values for what a later shorthand does not name, and a longhand after the shorthand.
- `a_background_position_reads_its_keywords_by_axis`.
- `a_shorthand_layers_url_is_taken_whole`: a `data:` URL holding `rgb(0,0,0)`, and a layer with no image.

Fail-first: the first two spliced into develop's test module, non-test code untouched (`scratch/s1002b/failfirst_layers.py`). The third calls the new function, so it cannot compile there.

```
test element_identity_tests::the_background_shorthand_sets_each_layers_position_size_and_repeat ... FAILED
thread 'element_identity_tests::the_background_shorthand_sets_each_layers_position_size_and_repeat' (16851005) panicked at crates/rustkit-engine/src/lib.rs:16461:9:
assertion `left == right` failed
  left: Repeat
 right: NoRepeat
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 301 filtered out; finished in 30.25s
error: test failed, to rerun pass `-p rustkit-engine --lib`
rc 101
test element_identity_tests::a_background_position_reads_its_keywords_by_axis ... FAILED
thread 'element_identity_tests::a_background_position_reads_its_keywords_by_axis' (16851508) panicked at crates/rustkit-engine/src/lib.rs:16525:9:
assertion `left == right` failed
  left: (Percent(1.0), Percent(0.5))
 right: (Percent(0.5), Percent(1.0))
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 301 filtered out; finished in 0.00s
error: test failed, to rerun pass `-p rustkit-engine --lib`
rc 101
restored: True
```

On the branch: `cargo test -p rustkit-engine --lib background` 8/8 (these three, #433's test and four older background tests). The whole engine suite was not run locally (the other lanes were building, load 13 to 23, and its GPU guard times out under load), so CI's `unit-suites` is its first full run.

## Receipt

Both arms were release-built from clean sources against the same target directory and run on the develop worktree's fixtures (`scratch/s1001c/camp3.py`). develop is `8f78611` (the same tree as develop `33d7368`); fix is `f7f82fc`.

```
all:      develop 2026-10-02T04:09:00 26/26 avg 1.1069 | fix 2026-10-02T04:59:26 26/26 avg 1.1069 | 26/26 identical
builtins: develop 2026-10-02T04:04:37  5/5  avg 1.8139 | fix 2026-10-02T04:55:20  5/5  avg 1.8139 | 5/5 identical
micro:    develop 2026-10-02T04:03:51 13/13 avg 0.6548 | fix 2026-10-02T04:54:37 13/13 avg 0.6548 | 13/13 identical
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): output identical on both arms, line for line.

<details><summary>Per-case diff_pct, scope all (develop | fix)</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6742 | 3.6742 |
| article-typography | 4.7857 | 4.7857 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| card-grid | 1.3034 | 1.3034 |
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

RustKit frames only, each site captured develop, fix, develop, fix at 1280x800 (`scratch/s1001g/ab.py`). "within" is the difference between two captures of the same binary (the site's own variance); "across" is one arm against the other, all four pairs.

```
google       within develop   0.03%  within fix   8.39%  across   0.03%   8.36%   0.00%   8.39%  failed: none  secs A,B,A,B 15,12,12,16
youtube      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 11,17,12,15
facebook     within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,30,30,30
instagram    within develop    nan%  within fix    nan%  across    nan%   0.00%    nan%    nan%  failed: B1,A2  secs A,B,A,B 20,30,30,18
wikipedia    within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 14,19,14,12
lyft         within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 17,17,16,16
reddit       within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 3,3,3,3
x            within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 9,7,6,8
linkedin     within develop   2.96%  within fix   2.64%  across   2.64%   0.00%   1.55%   2.96%  failed: none  secs A,B,A,B 8,7,7,8
yahoo        within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 17,16,16,17
bing         within develop   0.00%  within fix   0.24%  across   0.24%   0.00%   0.24%   0.00%  failed: none  secs A,B,A,B 6,5,5,5
walmart      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 8,8,8,9
microsoft    within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 15,8,8,10
apple        within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 21,20,22,27
netflix      within develop   4.92%  within fix  12.77%  across  12.80%   4.83%  12.63%   4.82%  failed: none  secs A,B,A,B 24,23,22,22
shopify      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 22,27,29,30
weather      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 12,10,11,11
github       within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,30,30,30
squarespace  within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,30,30,30
cnn          within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,30,30,30
```

No site moves between the arms. Fifteen were captured on all four runs: twelve are pixel-identical across all four pairs (bing differs by its own 0.24%), and google, linkedin and netflix each served a second variant of the page to one capture. I looked at each: google's second fix capture is the page without the promotional panel (the other three captures, both arms, are identical to 0.03%); netflix's first fix capture has different headline copy and a different button style, and its second fix capture matches develop's to netflix's usual 4.8%; linkedin's four captures differ from each other by 0 to 3% in both arms.

**Not verified: facebook, github, squarespace and cnn** did not finish within the binary's 30 s on either arm (load 13 to 21 from the other lanes; facebook was tried three times and took 13 to 16 s on a quiet machine an hour earlier). instagram finished on one capture of each arm, and that pair is identical. No check changes on any site that was captured.

So this is a correctness fix measured on the fixture, not a point: the first viewports of the board's sites do not use the forms it fixes with a gradient, and their url images are not fetched yet.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
