
## 2026-10-02 11:20

**Points: 26/60 -> 26/60 (no scoring run this session).** The number is the 05:51 daily quiet board on develop 97393a7: loads 16, readable 7, looks-right 3; scorable 25/57. The machine was at load 5 for the first twenty minutes and at 9 to 24 after that (the other lanes), so no Chrome oracle run was taken. Live-site evidence is RustKit frames A/B, scored where a site moved against the Chrome frames the 2026-10-01 14:54 quiet board stored.

**Atlas's 06:30 order:** (i) image fetches through the loader is #438, open. (ii) CSS background images, (iii) the census, (iv) glyph fallback and (v) L0 are not started. The weather paint regression of (ii) was not bisected. **LOADS on near-blank pages:** no rule change, per the 06:30 ruling; a page that paints a header and nothing else passes LOADS at 2% non-background pixels, which is an artifact of the instrument and not a sign the page works.

**PR opened (Prometheus R1 + Cursor R2; not mine to merge):**
- **#438** `atlas/rs-image-loader-routing` @ ef58355 (base develop e60ba68; one commit): **raster images are fetched through the resource loader.** An `<img>` went out through the image manager's own HTTP client: no Referer, past the request interceptor (the shield), with that client's defaults instead of the engine's user agent and cookie setting. The image crate now has no HTTP client at all, so there is no second way out; the engine fetches as it does stylesheets, fonts and SVG, under the subresource deadline.
  - **Tests:** two in the engine (headless). On develop's code both fail: of three images, all three were requested, none with a Referer, including the one the interceptor blocks. On the branch: two requests, each with the policy's Referer, the blocked one never sent. rustkit-image 20/20. The engine suite was 375 of 377 in a parallel run at load 8 to 13; the two failures are timing tests (Found 3).
  - **Campaign vs develop 60f7d39:** all 26/26, builtins 5/5, micro 13/13 identical; ratchet identical line for line. The develop arm is one merge behind the branch's base (#436, cascade lane); no third arm was built.
  - **Real sites, 20 run, 16 captured on both arms:** no frame changes because of the route (Found 1). Not captured within 30 s on either arm: github, squarespace; instagram and cnn on one capture of four.
  - **At 11:18:** CI running (audit and selector-key pass, the rest pending); no R1, no R2 stamp yet.

**#429 (the push button's default box): re-receipted, pushed, CI green, and STILL FLAGGED DRAFT** (decision 1). #435 MERGED at 08:18, so I merged develop 60f7d39 into the branch (2f2e463, pushed, no force) and rebuilt both arms.
  - **The two regressions that kept it in draft are gone.** weather: 0.83% -> 0.02% across arms (the "More" pill has no face; 206 pixels left on a header button that is grey on both arms). squarespace: 1.94% -> 0.00% (pixel-identical).
  - **Campaign vs develop 60f7d39:** all 25/26 identical, `form-controls` 3.2388 -> 2.8712% and its geometry gate 43 -> 32; builtins 5/5 identical; micro 12/13 identical (the same case). The fix arm's micro scope failed one capture under load on the first run and measured 13/13 on the second.
  - **Still off, measured and not fixed:** walmart 0.90% across arms (a grey pill face behind an icon button and a 2px frame on another; 91.05 -> 91.04% against Chrome); github 0.15% (the "Sign up" button's frame is grey where develop's is dark; 16.24 -> 16.28% against Chrome, 359 pixels worse).
  - Only weather, squarespace, walmart and github were re-captured at this head; the all-site A/B in the body's last bullet is the one from the previous head.
  - CI at 2f2e463: all green at 11:18 (`unit-suites` included).

**Found:**
1. **The Referer was not what kept any first-viewport image off the 20 board pages.** Sixteen sites captured on both arms: ten pixel-identical on all four pairs, facebook and apple identical on the pairs captured, and google, linkedin, netflix and yahoo differ by what the site served (linkedin and netflix send different headline copy from one request to the next; I looked at both frames). So #438 moves no check, and the missing images Pete sees have other causes; the census is how to find them.
2. **The image manager's client also ignored the engine's user agent and cookie setting.** Read in the code (it built its own client with defaults), not measured on a site. Fixed by the same change.
3. **`a_stalled_subresource_is_dropped_at_the_subresource_budget` fails on develop 60f7d39, alone, at load 8** (3.44 s against its 2.5 s limit, twice; 2.72 s on the branch). `scripts_are_fetched_while_the_subresources_load` failed in the parallel run and passed alone. Both are timing limits on a loaded machine; CI runs them quiet.
4. **`gh pr ready` is refused in the headless session** ("requires approval"), as are `git -C <other worktree>` and `sed -i`. The first one matters: decision 1.
5. **Release builds:** 7.5 minutes at load 5, 11.8 at load 10 to 15, about 17 at load 14 to 24. A 20-site A/B took 26 minutes at load 13.

**Not done:**
- The background-image fetch on top of #438, length positions, SVG backgrounds, the weather bisect.
- The image census, glyph fallback, L0, `::first-letter`.
- A develop arm at e60ba68 for #438; A/B rows for github, squarespace, instagram and cnn on #438.
- What clears walmart's pill face in Chrome; github's button frame colour.
- A scoring board.

**Next:** (1) Anything CI, R1 or R2 ask on #438 and #429. (2) The background-image fetch: merge develop and #438's branch into `atlas/rs-background-image-fetch`, add the Referer and blocked-URL assertions to its test, all-site A/B scored against Chrome in both directions, weather bisect inside it. (3) Length positions, then SVG backgrounds. (4) The image census. (5) A scoring board on a quiet machine (github's LOOKS RIGHT after #435 is the check to watch).

**Decisions for Pete:**
1. **#429 is ready but I cannot take the draft flag off.** The headless session is not allowed to run `gh pr ready`, and I did not go around that. The PR's first line says so. Either flip it (one click on #429) or allow `gh pr ready` for this lane; until then Prometheus may not pick it up.
2. **Four board sites cannot be captured while the other lanes build.** github, squarespace, cnn and instagram ran out the 30 s limit on both arms today, as squarespace, facebook and cnn did at 08:20, so the heaviest pages are missing from every A/B receipt taken at load 12 or more, and builds take twice as long. Recommendation: the three lanes agree a quiet half hour per session for receipts (this lane would take minutes 0 to 30 after the hour it starts), rather than raising the limit, which would hide real slowness.
