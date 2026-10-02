
## 2026-10-02 05:25

**Points: 26/60 -> 26/60 (no scoring run this session).** The number is the 2026-10-01 14:54 quiet board on develop 2b764be: loads 16, readable 7, looks-right 3; scorable 25/57. The machine was quiet for the first 25 minutes (load 2 to 5: a release build took 6.5 minutes) and at 12 to 23 for the rest (the other lanes), so no Chrome oracle run was taken. Live-site evidence is RustKit frames A/B.

**The 15:10 question (28 -> 26, instagram and microsoft)** is answered in the 2026-10-01 17:28 section and was not re-run.

**Queue status:** items 1 to 4 are MERGED. Item 5 (L0) is not built and waits on decision 1 of the 02:10 digest. `::first-letter` is not started.

**PRs opened (Prometheus R1 + Cursor R2; not mine to merge; #433 merged, #434 open):**
- **#433** `atlas/rs-background-shorthand-reset` @ 8f78611 (develop 2dd7680 merged in; commits a45dcb2 from last session and 8f78611): **the `background` shorthand resets the colour it does not name.** `background: none`, `0 0`, `unset`, `initial`, an image or a gradient alone all left an earlier colour painted, and a colour beside an image was dropped. The second commit drops a value in another engine's prefix (`-moz-`, `-o-`, `-ms-`) whole, as Chrome does, so the reset cannot clear a colour on a declaration Chrome ignores.
  - **Against Chrome 148** (fixture `scratch/s1002b/bgsh.html`, the colour painted in each of 18 boxes): 11 of 18 wrong on develop, 1 of 18 on the fix (`background: inherit`, not handled for any non-inherited property).
  - **Test:** one engine test; it **fails on develop's code** ("`background: none` clears the colour": red against transparent) and passes on the branch.
  - **Campaign vs develop 2dd7680:** all 25/26, builtins 5/5, micro 13/13 identical. The mover is `card-grid`, 1.3074 -> 1.3034%: a white background left under a later gradient showed as slivers at a card's rounded corners. Ratchet identical except that case's paint score.
  - **Real sites, 17 captured:** none moves between the arms. github, squarespace and cnn did not finish in 30 s on either arm (load).
  - **MERGED at 04:32** (develop 33d7368), after CI green (engine suite included), R1 DESIGN CLEAR and R2-STAMP PASS at 8f78611.
- **#434** `atlas/rs-background-shorthand-layers` @ f7f82fc (base: #433's head, the same tree as develop 33d7368): **the `background` shorthand sets each layer's position, size, repeat and boxes.** A layer kept only its image, and only when it began with it: `url(a.png) no-repeat center / cover` tiled from the corner, `no-repeat url(a.png)` had no image, and `linear-gradient(...) no-repeat 0 100% / 100% 2px` painted nothing. `background-position` also read `bottom` as the right edge and `top right` with the axes swapped.
  - **Against Chrome 148** (fixture `scratch/s1002b/bgsh3.html`, 20 boxes, a `data:` PNG and gradients): 18 of 20 boxes more than 1% off on develop, 1 of 20 on the fix (a url image at a length position: Found 3).
  - **Tests:** three engine tests; the two that can compile on develop **fail there** (`Repeat` against `NoRepeat`; `(100%, 50%)` against `(50%, 100%)` for `bottom`). `cargo test -p rustkit-engine --lib background` 8/8.
  - **Campaign vs develop:** all 26/26, builtins 5/5, micro 13/13 identical; ratchet identical line for line.
  - **Real sites, 15 captured on all four runs:** none moves between the arms (google, linkedin and netflix each served a second page variant to one capture; I looked at each). **Not verified: facebook, github, squarespace, cnn** (30 s limit on both arms, load 13 to 21; facebook tried three times).
  - **At 05:16:** CI all green at f7f82fc (`unit-suites`, the full engine suite, included), R1 DESIGN CLEAR; no R2 stamp yet; not merged.

**Branch pushed, NO PR: `atlas/rs-background-image-fetch`** (on top of #434's branch). See Found 1. @ 96fb04d: `load_images` also fetches the http(s) `url()` backgrounds the display list paints; one engine test (the image is requested and cached; a `display: none` box asks for nothing), which passes on the branch. No fail-first run, no release build, no campaign, no A/B. It is not a PR for a second reason: decision 2.

**#429 (the push button's default box) is still a DRAFT.** #433 was what it waited for. I merged develop into its branch locally and started its rebuild, then stopped the build: it was slowing the A/B that #434 needed and could not have finished its receipts inside the cap. The merge is NOT pushed (worktree `rs-l0`, local commit 396f68b). First thing next session.

**Found:**
1. **A CSS background image is never fetched.** Only `<img>` elements are looked for (`Engine::discover_images`). A `background-image: url(...)` reaches the display list and is painted only when it is a `data:` URL or happens to be in the cache already. Measured: a page of fifteen boxes with `url(dot.png)` backgrounds, served over HTTP; the server saw one request, for the HTML, and all fifteen boxes are empty (`scratch/s1002b/bgsh2.html`, Chrome's frame beside it). This is on develop and has been there since the image path was written. It is the "missing images and background images" of Pete's 2026-09-28 read of the board, and the image census that note asked for was never run, which is why it was not found then.
2. **The shorthand dropped everything but the image** (#434). It had to be fixed before the fetch: a `no-repeat` sprite that tiles is worse than no image.
3. **A url image at a length position sits at the corner.** The display list's image command carries the position as two fractions, so `10px 20px` (and every sprite offset) is dropped. Gradients take the length. Measured on #434's fixture, not fixed.
4. **A longhand that arrives before its layer is lost.** `background-size: cover` in one rule and `background-image` in a later one: the size is applied to zero layers. Read in the code, not measured, not fixed.
5. **An SVG background would be fetched and not painted:** only `<img>` commands are spliced from the SVG cache. The fetch branch leaves `.svg` backgrounds out for that reason.
6. **google's page, in three captures of four on both arms, has a conic-gradient panel painted as a rectangle over the search box** (`scratch/s1002b/google-b1b2.png`). Not looked into.
7. **`outline` did not show on the fixture's boxes** (`outline: 1px solid #ccc`). Seen in one frame, not measured.
8. **A release build was 6.5 minutes at load 3 to 5 and 38 minutes at load 13 to 23.** One develop-arm campaign capture failed under load and was re-run alone.

**Not done:**
- #429's re-receipt and un-draft (above).
- The fetch PR: receipts, the length position (Found 3), SVG backgrounds.
- L0, `unicode-range`, `::first-letter`, text fields and selects' default box, inset shadows, a real shadow blur, the emoji-presentation rule.
- A scoring board.

**Next:** (1) #429: push the merge, rebuild, confirm weather's button, re-receipt, mark ready. (2) Anything CI, R1 or R2 ask on #434. (3) The fetch PR: all-site A/B scored against Chrome's stored frames (this one should move sites, in both directions), then the length position and SVG backgrounds. (4) `unicode-range` or L0, per the 02:10 digest's decision 1.

**Decisions for Pete:**
1. **Background images before `unicode-range` and L0?** No CSS background image on any live site has ever painted (Found 1). The fix is small and it is the first change in several sessions that I expect to move LOOKS RIGHT on real sites, for better and, where a sprite lands at the wrong offset, for worse (Found 3 goes with it). Recommendation: yes, next, as two or three small PRs (fetch; length positions; SVG backgrounds), each with an all-site A/B against Chrome. The 02:10 question (`unicode-range` or L0 after that) stays open.
2. **Route image fetches through the resource loader BEFORE the fetch PR opens?** The branch as written sends background images down the path `<img>` uses, and I checked that path today: the image manager still has its own HTTP client, so raster images get no Referer, no shield and no subresource budget (the 2026-09-28 note decided to route `<img>` through the loader; it has not been done; SVG images already go through it). Fetching backgrounds that way would add requests the shield never sees. Recommendation: yes, the routing first, for `<img>` and backgrounds together, and I will not open the fetch PR on the unshielded path. Say so if you would rather it land and be routed after.
3. **The image census from the 2026-09-28 note (requested, fetched, decoded, painted, by cause) was never run and would have found this in an hour.** Recommendation: run it on the board's sites once the fetch lands, so the next gap in this path (formats, `image-set()`, lazy loading, `<picture>`) is found by counting, not by accident.
