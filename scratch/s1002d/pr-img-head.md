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
FAILFIRST
```

On the branch: both pass; `cargo test -p rustkit-image` 19 + 1 pass; `cargo test -p rustkit-engine --lib --features headless` 375 of 377 in a parallel run at load 8 to 13. The two failures are timing tests in `page_script_tests`. `scripts_are_fetched_while_the_subresources_load` passes when the module is run alone. `a_stalled_subresource_is_dropped_at_the_subresource_budget` (limit 2.5 s) took 2.72 s alone on the branch **and fails on develop 60f7d39 too, alone, twice (3.44 s)** at the same load, so it is the machine and not this change; CI's quiet run is the check.
