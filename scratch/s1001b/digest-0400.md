
## 2026-10-01 04:00: Atlas queue item 2 is #398. Web fonts were never painted with their own glyphs; WOFF/WOFF2 needed no decoder

**Points: 28/60 -> 28/60.** No scoring board run this session (load 13 to 25 throughout); the 22:48 quiet board on develop af4b95d stands (loads 17 · readable 8 · looks-right 3; scorable 27/57). #396 (queue item 1) merged at 01:23 with R1 CLEAR and R2 PASS; develop is now e4a82f7.

**PR opened (Prometheus R1 + Cursor R2; not mine to merge):**
- **#398 `atlas/rs-woff` @ fe23762** (base e4a82f7). Queue item 2.
  - **The review's premise was wrong for macOS.** Core Graphics already decodes WOFF and WOFF2, and the engine has no format filter. The `webfonts.rs` comment the review quoted was out of date. No decoder was added.
  - **What was actually broken:** a page is painted once before its fonts arrive, and the renderer's glyph cache was keyed by family *name*. The fallback's bitmaps were cached under the web font's name and reused on every later frame. Layout measured with the web font; paint drew Helvetica's glyphs at the web font's advances. Every format, TTF included. The same key let two documents that declare one family name with different files share glyphs.
  - **Fix:** the glyph key carries the identity (content hash) of the registered file, 0 for platform fonts. `webfonts::inspect` checks the sfnt / WOFF / WOFF2 header and table directory against the bytes received, with caps (32 MiB file, 128 MiB declared decoded size), before anything reaches Core Graphics.
  - **Tests:** an engine test serves Ahem as TTF, WOFF and WOFF2 over HTTP and reads the captured frame; it **fails on develop e4a82f7** and passes on the branch. Plus 1 renderer test, 9 container tests, 3 registry tests. rustkit-text 94/94, rustkit-renderer 91/91. The full engine suite was not run locally; CI is that evidence (pending when this was written).
  - **Campaign vs develop e4a82f7:** 26/26 and 5/5 identical, avg 1.1612 and 1.9072 on both arms, ratchet output identical. Expected: campaign pages load no web font, so the campaign cannot see this bug or this fix.

**Real-site A/B (asked for with this item):**
- RustKit frames, develop/fix/develop/fix, no Chrome: **shopify changes 4.48% and nothing else does.** Its hero headline was Helvetica glyphs squeezed together ("Bethenext"); it is now the site's own face, as in Chrome. Against the stored quiet-board Chrome frame: 19.61% -> 18.69% (still over 15). facebook, wikipedia, x, apple and lyft are pixel-identical between the arms. google differs only when Google serves a different search-box variant. github, netflix and squarespace were not measured (all captures failed at load 25).
- The scoring board A/B was **not usable**: 8 sites ran in both arms at about 3 minutes per site per arm, Chrome's screenshots timed out, and RustKit loads failed in the develop arm. Its totals (8 vs 14) are load noise and I am not claiming them. Runs kept under `trench/realsite/runs/20261001T0715Z-woff-ab-*`.

**Not done, plainly:**
- Queue item 3 (elliptical corners) and `::first-letter`: not started. Item 3 is mapped, not built: `scratch/s1001b/radius-map.md` (an agent's read-only sweep; its line numbers are unverified). Two things in it change the size of the item: box-shadow carries no corner radius at all today, and images and glyphs under a rounded `overflow: hidden` get only the rectangular clip.
- The select-width follow-up to #396 (settings is 0.01 worse on develop because of it): not started. It is not the small item last night's note said. Chrome's control sizes depend on `box-sizing` (list box 39 wide under `border-box`, 43 without; text input 149x19 vs 153x21), so the fix is a rule, not two constants.
- From the review's list for item 2: `font-display` timing, `unicode-range`, descriptor matching beyond weight and italic, per-document font handles, and any decoder for Windows and Linux.

**Found:**
1. Why five of the measured sites did not change is not established. Likely a document with external stylesheets defers its first layout, so no fallback paint happens before the fonts arrive. If so, the bug hit pages with inline `<style>` only, which is a smaller blast radius than "every page with web fonts".
2. `--html-file` pages cannot load a relative-URL web font (no document base). That is why the 26 campaign cases never showed this.
3. A full board A/B costs about 2 hours at night load. The frames-only A/B (`scratch/s1001b/frames_ab.py`, 4 captures per site, no Chrome) answers "did this change pixels on live sites" in about a minute per site and separates the site's own variance from the change.

**Next:** (1) quiet full board once #398 lands. (2) Item 3 from the map, commit sequence 1 to 6 in `radius-map.md`; steps 1 to 5 change no pixels, so one build can carry them. (3) `::first-letter`. (4) select sizing under `box-sizing`.

**Decisions for Pete:**
1. **WOFF2 decoding: keep the OS path on macOS?** It works today and #398 puts header and size checks in front of it. It does not sanitise table contents the way Chrome does before handing a font to the OS. An in-engine decoder is needed for Windows and Linux anyway and needs Brotli (a pure-Rust crate, or our own). Recommendation: ship #398 as is; make the cross-platform decoder its own item and decide the Brotli dependency then.
2. **Build contention, again.** Load was 13 to 25 all session with three lanes on this Mac. Release builds took 12 and 16 minutes (6 quiet), Chrome's oracle screenshots timed out, and the board A/B was unusable. Staggering the lanes, or giving the board a protected slot, is still open from last night.
3. **Item 3's scope.** As written it covers "clip, border and shadow". Shadow corners and rounded clipping of images are not there at all today, so they are new work, not a representation change. Recommendation: one PR for the representation plus fill, border and clip (the queue item), a second for shadow corners and rounded image clips.
