## What

A CSS background image over http(s) was never fetched. `Engine::discover_images` looks for `<img>` elements only, so a `background: url(...)` reached the display list and waited for a cache entry nothing made. Only a `data:` URL painted (it is decoded at upload). No live site's CSS background image has painted on any board run.

This is step (ii) of the image work Atlas ordered on 2026-10-02, on top of #438 (raster image fetches go through the resource loader), so a background image goes out exactly as an `<img>` does: through the loader, past the shield, with the policy's Referer, under the subresource deadline. There is no second route.

## Change

`crates/rustkit-engine` only:

- `Engine::discover_background_images(display_list)` collects the http(s) URLs of the `DisplayCommand::BackgroundImage` commands the view paints. The display list is the place to look: its URLs are already absolute, and a box that is not rendered (`display: none`) has no command, as Chrome fetches no background for it.
- `Engine::load_images` adds them to the `<img>` list it already walks, so they are fetched by `fetch_raster_image` (#438) with no further change: one request per URL, eight at a time.

Left out on purpose, each a follow-up:

- **SVG backgrounds** (`.svg` URLs are skipped): only `<img>` commands are spliced from the SVG cache, so one would be fetched and never painted.
- **Length positions** (`10px 20px`, `right 5px`, sprite offsets): the command carries the position as two fractions, so a length is dropped and the image sits at the box's corner. Branch `atlas/rs-background-length-position` has the fix and goes up next; rows e5 and e13 below are that bug.

## Tests

`background_image_fetch_tests::a_css_background_image_is_fetched` (`rustkit-engine`, headless feature), against a local server that records each request's path and Referer. The page has a same-origin background (used by two boxes), a cross-origin one, one the interceptor blocks, and one on a `display: none` box. Exactly two requests arrive: the same-origin one with the full URL as Referer (no fragment), the cross-origin one with the origin only. The blocked one is never sent, the hidden box asks for nothing, and the fetched image is in the cache paint reads.

Fail-first: the module appended to develop's `lib.rs` (f0d5fa2), develop's code otherwise untouched (`scratch/s1002e/failfirst.py fetch` on the hub branch):

```
thread 'background_image_fetch_tests::a_css_background_image_is_fetched' panicked at crates/rustkit-engine/src/lib.rs:28424:9:
assertion `left == right` failed: one request per background the shield allows, each with the policy's Referer; none for the blocked one, none for a box that is not rendered
  left: []
 right: [("/cross.png", Some("http://127.0.0.1:63390/")), ("/shown.png", Some("http://127.0.0.1:63390/page?q=1"))]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 377 filtered out
```

On the branch:

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 377 filtered out; finished in 6.26s
```

## Against Chrome 148: fixture

`scratch/s1002b/bgsh2.html` on the hub branch: fifteen 120x60 boxes with a `url(dot.png)` background, served over HTTP, each box's share of pixels more than 16/255 off pinned Chrome's frame (`scratch/s1002b/box_cmp.py`).

```
e0    develop   5.56%  fix   0.00%  |  background-image: url(dot.png); background-repeat: no-repeat; background-position: center
e1    develop   5.56%  fix   0.00%  |  background: url(dot.png) no-repeat
e2    develop   5.56%  fix   0.00%  |  background: url(dot.png) no-repeat center
e3    develop   5.56%  fix   0.00%  |  background: url(dot.png) no-repeat right bottom
e4    develop  33.33%  fix   0.00%  |  background: url(dot.png) repeat-x
e5    develop  16.67%  fix  29.17%  |  background: url(dot.png) no-repeat 10px 20px / 40px 30px
e6    develop   5.56%  fix   0.00%  |  background: #ff0 url(dot.png) no-repeat center
e7    develop   0.00%  fix   0.00%  |  background: linear-gradient(blue, blue) no-repeat 20px 10px / 30px 15px
e8    develop 100.00%  fix   0.00%  |  background: url(dot.png) center / cover no-repeat
e9    develop  50.00%  fix   0.00%  |  background: url(dot.png) center / contain no-repeat #eee
e10   develop 100.00%  fix   0.00%  |  background: url(dot.png)
e11   develop   5.56%  fix   0.00%  |  background: no-repeat center url(dot.png)
e12   develop   5.56%  fix   0.00%  |  background: url(dot.png) 50% 50% no-repeat, linear-gradient(#cfc, #cfc)
e13   develop   5.56%  fix   6.60%  |  background: url(dot.png) no-repeat; background-position: right 5px bottom 5px
e14   develop 100.00%  fix   0.00%  |  background-position: center; background-repeat: no-repeat; background: url(dot.png)
boxes more than 1% off Chrome: develop 14 of 15, fix 2 of 15
```

e5 and e13 are the length positions named above: the image is now there and sits at the corner, where develop painted nothing. e5 is worse than develop by that measure (16.67 -> 29.17%) until the length-position branch lands.

A second served page (`scratch/s1002e/ext/`: the rules in an external sheet one directory down, with `../img/` URLs) confirms what is and is not asked for: a relative URL resolves against the sheet, and a `var()` value, a `::before` box, a rule under `@media`, a layer beside a gradient, a root-relative URL and an inline `style` are all fetched. **`image-set()` and `-webkit-image-set()` are not**: no request, no image (not handled by the background parser; not in this PR).

## Campaign receipt

Arms: develop **f0d5fa2** (the branch's merge base) vs fix **91af4e9**, both release binaries built in this session through `cargo-serial` with every workspace source touched first. (The fix binary was built at the merge commit 42e9f7a plus the test-only change that 91af4e9 commits.) develop has since moved to e006c68 (#440, #441); not merged in.

```
all:      develop 2026-10-02T13:03:38 26/26 avg 1.1069 | fix 2026-10-02T13:55:16 26/26 avg 1.1069 | 26/26 identical
builtins: develop 2026-10-02T13:00:03 5/5 avg 1.8139 | fix 2026-10-02T13:09:12 5/5 avg 1.8139 | 5/5 identical
micro:    develop 2026-10-02T12:59:15 13/13 avg 0.6548 | fix 2026-10-02T13:08:30 13/13 avg 0.6548 | 13/13 identical
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): identical line for line on both arms (28 lines).

<details><summary>Per-case diff_pct (all 26), develop f0d5fa2 vs fix 91af4e9</summary>

| case | develop | fix |  |
|---|---|---|---|
| about | 3.6742 | 3.6742 |  |
| article-typography | 4.7857 | 4.7857 |  |
| backgrounds | 1.2163 | 1.2163 |  |
| bg-pure | 0.0000 | 0.0000 |  |
| bg-solid | 0.2106 | 0.2106 |  |
| card-grid | 1.3034 | 1.3034 |  |
| chrome_rustkit | 1.1234 | 1.1234 |  |
| combinators | 0.6547 | 0.6547 |  |
| css-selectors | 1.3299 | 1.3299 |  |
| flex-positioning | 0.6327 | 0.6327 |  |
| form-controls | 3.2388 | 3.2388 |  |
| form-elements | 0.9535 | 0.9535 |  |
| gpu-gradient-regression | 0.3903 | 0.3903 |  |
| gradient-backgrounds | 1.0133 | 1.0133 |  |
| gradient-no-radius | 0.5204 | 0.5204 |  |
| gradient-radius-only | 0.3429 | 0.3429 |  |
| gradients | 0.1412 | 0.1412 |  |
| image-gallery | 0.5217 | 0.5217 |  |
| images-intrinsic | 0.3267 | 0.3267 |  |
| new_tab | 1.1019 | 1.1019 |  |
| pseudo-classes | 0.4341 | 0.4341 |  |
| rounded-corners | 0.4354 | 0.4354 |  |
| settings | 2.0919 | 2.0919 |  |
| shelf | 1.0781 | 1.0781 |  |
| specificity | 0.6015 | 0.6015 |  |
| sticky-scroll | 0.6575 | 0.6575 |  |

</details>

<details><summary>Per-case diff_pct (builtins), develop f0d5fa2 vs fix 91af4e9</summary>

| case | develop | fix |  |
|---|---|---|---|
| about | 3.6742 | 3.6742 |  |
| chrome_rustkit | 1.1234 | 1.1234 |  |
| new_tab | 1.1019 | 1.1019 |  |
| settings | 2.0919 | 2.0919 |  |
| shelf | 1.0781 | 1.0781 |  |

</details>

<details><summary>Per-case diff_pct (micro), develop f0d5fa2 vs fix 91af4e9</summary>

| case | develop | fix |  |
|---|---|---|---|
| backgrounds | 1.2163 | 1.2163 |  |
| bg-pure | 0.0000 | 0.0000 |  |
| bg-solid | 0.2106 | 0.2106 |  |
| combinators | 0.6547 | 0.6547 |  |
| form-controls | 3.2388 | 3.2388 |  |
| gpu-gradient-regression | 0.3903 | 0.3903 |  |
| gradient-no-radius | 0.5204 | 0.5204 |  |
| gradient-radius-only | 0.3429 | 0.3429 |  |
| gradients | 0.1412 | 0.1412 |  |
| images-intrinsic | 0.3267 | 0.3267 |  |
| pseudo-classes | 0.4341 | 0.4341 |  |
| rounded-corners | 0.4354 | 0.4354 |  |
| specificity | 0.6015 | 0.6015 |  |

</details>

The fix arm's first `all` run failed one capture under load (`chrome_rustkit`, NOT-MEASURED); the scope was re-run alone and is the row above. The campaign's pages use local files and `data:` images, so this says nothing else moved, not that the fetch is right; the test and the fixture are for that.

## Real sites: no board site moves, and why

RustKit frames, develop / fix / develop / fix on each of the board's 20 live URLs at 1280x800 (`scratch/s1001g/ab.py` on the hub branch), load 12 to 25 with the other lanes building. "Within" is a site's own variance between two captures of one arm; "across" is the four develop-fix pairs.

```
google       within develop   4.09%  within fix   0.00%  across   0.00%   0.00%   4.09%   4.09%  failed: none  secs A,B,A,B 16,12,13,13
youtube      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 16,13,14,14
facebook     within develop    nan%  within fix    nan%  across    nan%    nan%   0.00%    nan%  failed: A1,B2  secs A,B,A,B 30,27,30,30
instagram    within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,30,30,30
wikipedia    within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 29,19,23,18
lyft         within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 23,24,12,13
reddit       within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 5,4,4,5
x            within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 10,9,9,10
linkedin     within develop   2.96%  within fix  40.81%  across   2.96%  41.05%   0.00%  40.81%  failed: none  secs A,B,A,B 11,12,12,13
yahoo        within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 23,23,24,24
bing         within develop   0.24%  within fix   0.00%  across   0.00%   0.00%   0.24%   0.24%  failed: none  secs A,B,A,B 7,7,10,8
walmart      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 10,10,10,11
microsoft    within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 11,14,19,21
apple        within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,30,30,30
netflix      within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,30,30,30
github       within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,30,30,30
shopify      within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,30,30,30
squarespace  within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,30,30,30
cnn          within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,32,30,32
weather      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 19,13,13,17
```

Second attempt at four of the sites that ran out the 30 s limit:

```
netflix      within develop    nan%  within fix   2.27%  across    nan%    nan%   2.29%   2.17%  failed: A1  secs A,B,A,B 30,27,25,24
apple        within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 27,25,28,26
shopify      within develop    nan%  within fix    nan%  across   0.00%    nan%    nan%    nan%  failed: A2,B2  secs A,B,A,B 27,22,30,31
github       within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 31,30,30,30
```

- **Pixel-identical across arms on every pair captured:** google (A1 = B1 = B2; A2 is another page of its own), youtube, wikipedia, lyft, reddit, x, yahoo, bing (same), walmart, microsoft, weather, apple (second attempt), facebook and shopify (one pair each).
- **linkedin:** B2 is the site's other page variant ("Welcome to your professional community", 40.8% from the other three, which are within 2.96% of each other across arms). Its own variance; I looked at the frames.
- **netflix:** 2.17 to 2.29% across, 2.27% within the fix arm: its own variance.
- **Not captured inside the binary's 30 s on either arm:** instagram, github, squarespace, cnn (the heaviest pages; the machine was at load 12 to 25).
- Against the stored Chrome frames (`scratch/s1001g/vs_chrome.py`), every site captured scores the same on both arms.

**Why nothing moves** (measured, `scratch/s1002e/bg_census.py`: the `background_image` commands in each site's display list on the fix binary): on google, lyft, x, linkedin, weather, microsoft, reddit, youtube and walmart there is **no** `url()` background command at all; bing has one, a `data:` SVG; wikipedia has 99, all http SVG (external-link icons, none in the first viewport). So on the eleven board sites censused there is not one http raster background for this PR to fetch. Either those pages have none in Chrome either, or the cascade drops them before the display list (`image-set()` is one such form, shown above). Which, per site, needs Chrome's side of the count; that is the image census, next.

So this PR is a correctness fix with a measured null on the board, not the visible change the 05:25 digest expected. The weather page is pixel-identical across arms on all four pairs.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
