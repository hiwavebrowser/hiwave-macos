
## 2026-10-02 14:15

**Points: 26/60 -> 26/60 (no scoring run this session).** The number is the 05:51 daily quiet board on develop 97393a7: loads 16, readable 7, looks-right 3; scorable 25/57. The machine was at load 4 for the first ten minutes and at 11 to 37 after that (the other lanes), so no Chrome oracle run was taken. Live-site evidence is RustKit frames A/B, scored against the Chrome frames the 2026-10-01 14:54 quiet board stored.

**Atlas's 06:30 order:** (i) image fetches through the loader, #438, **MERGED 11:35** (develop f0d5fa2). (ii) CSS background images: the fetch is **#443, open**; length positions are written, tested and pushed as a branch without a PR; SVG backgrounds are not started. (iii) the census: a first half was run (RustKit's side, 13 sites) because the A/B needed explaining; Chrome's side is not done. (iv) glyph fallback and (v) L0 are not started. **The weather paint regression:** weather is pixel-identical between develop and the fetch on all four pairs, and its display list has no `url()` background at all, so nothing in (ii) touches it. I could not tell from the plan and digests which regression "Found 1 of the drop" names (decision 3). **LOADS on near-blank pages:** no rule change, per the 06:30 ruling; it is an artifact of the instrument (a header and nothing else passes at 2% non-background pixels).

**PR opened (Prometheus R1 + Cursor R2; not mine to merge):**
- **#443** `atlas/rs-background-image-fetch` @ 91af4e9 (develop f0d5fa2 merged in; the PR's diff is 147 lines, engine only): **a CSS background image is fetched.** `load_images` adds the http(s) `url()` backgrounds of the display list to the `<img>` list, so they go out through `fetch_raster_image` (#438): the loader, the shield, the policy's Referer, the subresource deadline. No second route.
  - **Test:** one engine test (headless) against a recording server: a same-origin background used by two boxes is asked for once with the full URL as Referer, a cross-origin one with the origin only, a blocked one and one on a `display: none` box not at all. **On develop's code it fails** (no request arrives: `left: []`); on the branch it passes.
  - **Against Chrome 148** (fixture `scratch/s1002b/bgsh2.html`, fifteen boxes served over HTTP): 14 of 15 boxes more than 1% off on develop, **2 of 15 on the fix**. The two are length positions (the branch below). One of them is worse than develop by this measure (16.67 -> 29.17%): the image is now there, at the corner.
  - **Campaign vs develop f0d5fa2:** all 26/26, builtins 5/5, micro 13/13 identical; ratchet identical line for line. The fix arm's first `all` run failed one capture under load and the scope was re-run alone.
  - **Real sites, 20 run, 16 captured on at least one pair: none moves** (Found 1). Not captured within 30 s on either arm: instagram, github, squarespace, cnn.
  - CI had not reported when the session stopped; no R1, no R2 stamp yet.

**Branch pushed, NO PR: `atlas/rs-background-length-position` @ b5a08a5** (base develop f0d5fa2, independent of #443). A `url()` background at a length position (`10px 20px`, a sprite's `-20px -40px`, `right 10px bottom 5px`) sat at the box's corner: the display-list command carried only two shares of the free space. It now carries a pixel offset, the renderer adds it, and the parser keeps an offset from the far edge. A frame test (seven pixels over three boxes) **fails on develop's code** (blue where the image should be) and passes on the branch; rustkit-css 49/49, rustkit-layout 602/602, rustkit-renderer 114/114. It has no release build, no campaign and no A/B, which is why it is not a PR: a release build was 19 to 21 minutes today.

**#429 (the push button's default box) is still flagged DRAFT** at 2f2e463; nothing changed on it this session (decision 1 of the 11:20 digest stands).

**Found:**
1. **The fetch moves no board site, because no board site's display list asks for one.** I counted the `background_image` commands in each site's display list on the fix binary: google, lyft, x, linkedin, weather, microsoft, reddit, youtube, walmart, netflix and shopify have **none**; bing has one, a `data:` SVG; wikipedia has 99, all http SVG (external-link icons, none in the first viewport). Thirteen sites, zero http raster backgrounds. So what I wrote at 05:25 ("the first change in several sessions that I expect to move LOOKS RIGHT") was wrong, and the 06:30 note that this is the most visible fix on the board rests on it. The missing backgrounds Pete sees are lost before the display list, or are not CSS backgrounds.
2. **`image-set()` and `-webkit-image-set()` backgrounds are dropped** (measured on a served page: no request, no image). Everything else I tried from an external stylesheet is fetched: a URL relative to the sheet, a `var()` value, a `::before` box, a rule under `@media`, a layer beside a gradient, a longhand in an earlier rule. This is one candidate for Found 1; I have not measured how many board sites use it.
3. **The serialising build wrapper worked, and it showed the cost of a cold target.** `rs-cargo.py` points at a new shared target (`rs-target`), which did not exist: the develop arm took 21 minutes, the fix arm 19, and my test build waited about 34 minutes for the build lock (another lane held it) and then compiled for 18. The whole session had room for two release arms and one debug build.
4. **Develop moved to e006c68 during the session** (#440 `[profile.parity]`, #441). My arms are at f0d5fa2, so both were `--release`.

**Not done:**
- Receipts and PR for the length-position branch; SVG backgrounds; `image-set()`.
- Chrome's side of the image census (what Chrome paints as a background per site), and the `<img>` half of it.
- The weather bisect (decision 3). Glyph fallback, L0, `::first-letter`.
- A/B rows for instagram, github, squarespace and cnn on #443.
- A scoring board.

**Next:** (1) Anything CI, R1 or R2 ask on #443. (2) Receipts for `atlas/rs-background-length-position` (merge develop in so it builds with `--profile parity`), then its PR. (3) The census, Chrome's side first: per board site, the elements in the first viewport whose computed `background-image` has a `url(`, beside RustKit's display list, so each missing background has a cause (`image-set()`, SVG, a dropped declaration, or not a background at all). (4) SVG backgrounds (wikipedia's 99, bing's one) and `image-set()`, in the order the census gives.

**Decisions for Pete:**
1. **The census before any more background work?** Found 1 says the fetch was not what kept backgrounds off the board, and I only found that by building it. Recommendation: yes. Length positions go up as a PR because they are written and tested, and then the census (both sides) decides between SVG backgrounds, `image-set()` and whatever else it shows, instead of my order from 05:25.
2. **Still open from 11:20: #429 is ready and I cannot take its draft flag off** (`gh pr ready` is refused in the headless session). One click, or allow that command for this lane.
3. **Which weather regression did the 06:30 note mean?** The fetch does not touch weather (identical frames, no background commands). If it is the "More" pill from #429, that was fixed by #435 and re-receipted at 11:20. If it is something else, name the frame or the digest line and I will bisect it first thing.
