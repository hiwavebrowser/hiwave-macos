## What

An `<img>` was fetched by the image manager's own HTTP client, not by the engine's resource loader. So a raster image request:

- carried **no Referer** (hotlink-protected CDNs refuse that; stylesheets, fonts, scripts and SVG images have sent the policy's Referer since #296);
- **never passed the request interceptor**, which is where the shield decides: a blocked tracker pixel was fetched anyway;
- used that client's defaults, not the engine's configured user agent and cookie setting.

This is the first step of the image work Atlas ordered on 2026-10-02 (routing first, then CSS background images on top of it), so that background images do not add requests the shield never sees.

## Change

`crates/rustkit-image`:

- The crate no longer depends on `rustkit-http` and `ImageManager` has no client, so there is no second way out for an image URL. `ImageError::NetworkError` goes with it (nothing matched on it).
- `ImageManager::insert_fetched(url, content_type, body)` decodes a body the caller fetched and caches it under the URL; an `image/svg+xml` body is handed back as `ImageError::Svg` as before.
- `load` and `load_blocking` answer from the cache or decode a `data:` URL; any other URL is an error. `load_blocking` no longer builds a runtime.

`crates/rustkit-engine`:

- `fetch_raster_image` fetches with `ResourceLoader::fetch` as `RequestDestination::Image`, with the document's referrer and policy, and gives the body to the image manager.
- `Engine::load_images` (the page load path) and `Engine::load_image` use it, under the subresource deadline as before. `load_images` now makes one request per URL however many elements show it (the image manager's pending list used to do that).

No one outside the engine called the image manager's network path (`rustkit-dom` uses the crate's srcset parser only).

Behaviour that changes with the route, beyond Referer and the shield: image requests send the engine's user agent, follow its cookie setting, and successful responses go through the loader's memory cache like other subresources.

## Tests

Two new, in `rustkit-engine` (`image_loader_routing_tests`, headless feature), against a local server that records each request:

- `an_img_is_fetched_through_the_loader_with_its_referer_and_the_shield`: a page with a same-origin image (twice), a cross-origin image and one the interceptor blocks. Exactly two requests arrive: the same-origin one with the full URL as Referer (no fragment), the cross-origin one with the origin only; the blocked one is never sent; the fetched image is cached at 4 x 4.
- `the_image_manager_does_not_fetch`: `load` and `load_blocking` on an uncached http URL are errors and the server sees nothing.

Fail-first: the module appended to develop's `lib.rs`, develop's code otherwise untouched (`scratch/s1002d/failfirst_img.py` on the hub branch):

```
thread 'image_loader_routing_tests::the_image_manager_does_not_fetch' (17249468) panicked at crates/rustkit-engine/src/lib.rs:28196:9:
assertion failed: manager.load_blocking(url.clone()).is_err()
thread 'image_loader_routing_tests::an_img_is_fetched_through_the_loader_with_its_referer_and_the_shield' (17249467) panicked at crates/rustkit-engine/src/lib.rs:28175:9:
assertion `left == right` failed: one request per image the shield allows, each with the policy's Referer; none for the blocked one
left: [("/cross.png", None), ("/same.png", None), ("/tracker.png", None)]
right: [("/cross.png", Some("http://127.0.0.1:63216/")), ("/same.png", Some("http://127.0.0.1:63216/page?q=1"))]
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 374 filtered out; finished in 9.02s
```

On the branch: both pass; `cargo test -p rustkit-image` 19 + 1 pass; `cargo test -p rustkit-engine --lib --features headless` 375 of 377 in a parallel run at load 8 to 13. The two failures are timing tests in `page_script_tests`. `scripts_are_fetched_while_the_subresources_load` passes when the module is run alone. `a_stalled_subresource_is_dropped_at_the_subresource_budget` (limit 2.5 s) took 2.72 s alone on the branch **and fails on develop 60f7d39 too, alone, twice (3.44 s)** at the same load, so it is the machine and not this change; CI's quiet run is the check.

## Campaign receipt

Arms: develop **60f7d39** vs fix **ef58355**, both release binaries built in this session with every workspace source touched first. The branch's base is develop e60ba68, which is 60f7d39 plus #436 (a cascade-lane change to the selector match key), so the fix arm carries #436 and the develop arm does not; I did not build a third arm at e60ba68.

```
all:      develop 2026-10-02T10:05:03 26/26 avg 1.1069 | fix 2026-10-02T10:48:08 26/26 avg 1.1069 | 26/26 identical
builtins: develop 2026-10-02T10:01:11 5/5 avg 1.8139 | fix 2026-10-02T10:44:35 5/5 avg 1.8139 | 5/5 identical
micro:    develop 2026-10-02T10:00:33 13/13 avg 0.6548 | fix 2026-10-02T10:43:53 13/13 avg 0.6548 | 13/13 identical
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): identical line for line on both arms (28 lines).

<details><summary>Per-case diff_pct (all 26), develop 60f7d39 vs fix ef58355</summary>

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

<details><summary>Per-case diff_pct (builtins), develop 60f7d39 vs fix ef58355</summary>

| case | develop | fix |  |
|---|---|---|---|
| about | 3.6742 | 3.6742 |  |
| chrome_rustkit | 1.1234 | 1.1234 |  |
| new_tab | 1.1019 | 1.1019 |  |
| settings | 2.0919 | 2.0919 |  |
| shelf | 1.0781 | 1.0781 |  |

</details>

<details><summary>Per-case diff_pct (micro), develop 60f7d39 vs fix ef58355</summary>

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

The campaign's pages load local files and `data:` images, so this says the decode and cache path is unchanged, not that the network route is right; the tests and the live A/B are for that.

## Real sites

RustKit frames, develop / fix / develop / fix on each of the board's 20 live URLs at 1280x800 (`scratch/s1001g/ab.py` on the hub branch), load 12 to 14 with the other lanes building. "Within" is a site's own variance between two captures of one arm; "across" is the four develop-fix pairs; the share of pixels where any channel differs by more than 8. `parity-capture` installs no shield, so this measures the Referer, the user agent and the cookie setting on image requests, not blocking.

```
google       within develop   4.09%  within fix   8.49%  across   8.49%   0.00%   8.88%   4.09%  failed: none  secs A,B,A,B 16,12,12,12
youtube      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 14,14,13,14
facebook     within develop    nan%  within fix   0.00%  across    nan%    nan%   0.00%   0.00%  failed: A1  secs A,B,A,B 30,28,29,29
instagram    within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: B1,A2,B2  secs A,B,A,B 28,30,30,30
wikipedia    within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 15,15,14,16
lyft         within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 18,20,25,19
reddit       within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 5,5,4,4
x            within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 9,10,9,9
linkedin     within develop   0.00%  within fix   0.79%  across   3.76%   2.96%   3.76%   2.96%  failed: none  secs A,B,A,B 15,12,15,11
yahoo        within develop   0.00%  within fix  24.58%  across  24.58%   0.00%  24.58%   0.00%  failed: none  secs A,B,A,B 23,14,24,22
bing         within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 7,8,7,7
walmart      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 12,11,10,10
microsoft    within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 12,10,12,12
apple        within develop   0.00%  within fix    nan%  across   0.00%    nan%   0.00%    nan%  failed: B2  secs A,B,A,B 21,25,26,30
netflix      within develop    nan%  within fix   4.89%  across    nan%    nan%  12.96%  12.86%  failed: A1  secs A,B,A,B 30,30,22,25
github       within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,30,30,30
shopify      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 22,26,26,27
squarespace  within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,B1,A2,B2  secs A,B,A,B 30,30,30,30
cnn          within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  failed: A1,A2,B2  secs A,B,A,B 30,29,30,30
weather      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  failed: none  secs A,B,A,B 16,12,11,10
```

**No site's frame changes because of the route.** Every pair that could be compared is either pixel-identical or differs by what the site itself served:

- Pixel-identical on all four pairs: youtube, wikipedia, lyft, reddit, x, bing, walmart, microsoft, shopify, weather. facebook (three captures) and apple (three captures) are identical on the pairs captured.
- **google:** one develop capture and one fix capture are pixel-identical (A1 = B2); the other two are different pages of its own (4.09% within develop). Its own variance.
- **linkedin:** 2.96 to 3.76% across. The two arms got different copy from the server: the develop captures say "Discover new opportunities" with "Sign in / Join now" in the header, the fix captures "Welcome to your professional community" with "Join now / Sign in" (`scratch/s1002d/linkedin-ab.png`). The hero illustration is the same in both. Against the stored Chrome frames: develop 19.87%, fix 19.75% and 20.20%.
- **netflix:** 12.9% across on the two pairs captured, 4.89% within the fix arm. Different headline copy again ("Endless entertainment starts at $8.99/mo" against "Unlimited movies, TV shows, and more", and a white against a red button: `scratch/s1002d/netflix-ab.png`). Against Chrome: develop 65.2%, fix 66.1 to 66.2%.
- **yahoo:** one fix capture is a different page (24.58% from the other three, which are identical to each other across arms).
- Not captured inside the binary's 30 s on either arm: github, squarespace (four of four), instagram (three of four), cnn (three of four). The machine was at load 12 to 14; these are the heaviest pages on the board.

So the Referer was not what kept any first-viewport image off these 20 pages today. That is a measured null, and it is the expected one for most CDNs; the change is for the shield and for the background-image fetch that goes on top of it. No scoring board was run.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
