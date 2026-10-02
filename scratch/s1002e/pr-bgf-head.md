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
FAILFIRST
```

On the branch:

```
ONBRANCH
```

## Against Chrome 148: fixture

`scratch/s1002b/bgsh2.html` on the hub branch: fifteen 120x60 boxes with a `url(dot.png)` background, served over HTTP, each box's share of pixels more than 16/255 off pinned Chrome's frame (`scratch/s1002b/box_cmp.py`).

```
FIXTURE
```

