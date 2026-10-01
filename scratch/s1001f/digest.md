
## 2026-10-01 __STAMP__

**Points: 26/60 -> 26/60 (no scoring run this session).** The number is today's 14:54 quiet board on develop 2b764be: loads 16, readable 7, looks-right 3; scorable 25/57. Machine load was 12 to 35 for the whole session (other lanes building), so no Chrome oracle run was taken; live-site evidence is RustKit frames A/B.

**Why the quiet board went 28 -> 26 between 05:29 and 14:54 (both points are LOADS, neither is #411):**
- **microsoft 1 -> 0.** It has been "blank frame (1.08%)" on every quiet board since 09-29. The 05:29 run is the outlier: the page came back unstyled (8.8% ink, all the nav links as plain text), which reads as a stylesheet that did not arrive that once. It is back to its usual frame.
- **instagram 1 -> 0.** #407 (image alpha) made the splash logo correct: it was a filled gradient square (2.08% ink, just over the 2% blank threshold) and is now Chrome's outlined glyph (0.59%). The frame is closer to Chrome's and the check counts it blank. The page is a JS-rendered shell either way (JS lane). No threshold was touched.

**Queue status:** items 1 to 4 are MERGED (#396, #398, #401 + #407, #411), and #412 MERGED at 14:36 (develop a0583fa). Item 5 (L0) is not started; see decision 1. `::first-letter` is not started.

**PR opened (Prometheus R1 + Cursor R2; not mine to merge):**
- **#417** `atlas/rs-generic-families` @ 5ad75d9 (base a0583fa), two commits: **the default font and the generic families are Chrome's faces on macOS.** Unstyled text is Times (it was the system font), `sans-serif` is Helvetica (system font), `monospace` is Courier (Menlo), `cursive` Apple Chancery, `fantasy` Papyrus, and the family appended to every author list is the UA default. Layout now measures all 15 rows of the fixture within 0.01px of Chrome 148 (unstyled 199.52, sans-serif 217.02, monospace 268.84). It also adds Blink's macOS ascent adjustment for Times, Helvetica and Courier (a 16px line is 18px in Chrome and was 16 here, which was already wrong for pages naming those fonts). Three tests, each shown failing first. Campaign 24/26 identical, `new_tab` 1.5685 -> 1.5003, `form-controls` 3.2405 -> 3.2388; ratchet holds on both arms. __CI__

**Found:**
1. **The first commit crashed x.com, and the real-site A/B is what caught it.** `CTFont::family_name` panics on a face with no family name, and x's downloaded font has none. Unit tests, the campaign and the fixture were all green. Fixed in the second commit with a test (Ahem with its `name` table hidden). All receipts are at the second commit; x is pixel-identical to develop on it.
2. **wikipedia's face is now Chrome's and its board number does not move** (15.15% -> 15.28% against the stable Chrome capture). Its 2938 text commands are Helvetica now; the diff is the page's layout (no sidebar, no banner). So this PR is a correctness fix measured on the fixture, not a point.
3. **Glyph fallback does not ask Core Text.** `<kbd>⌘</kbd>`, `←`, `→` on `about`: Courier has no glyph, Chrome's fallback face has Menlo's advance (21.23px box), RustKit's fixed list tries Apple Symbols first (22.3 to 22.6px). Geometry gate `about` 66 -> 73 boxes on #417 (the ratchet still holds). Menlo as the primary face used to hide it.
4. **`new_tab` shortcut rows:** the `<kbd>` of the third shortcut now sit 5px high (419.5 vs Chrome 424.5), as the second and eighth already do on develop (gate 15 -> 17). A pre-existing row-height or wrap difference that Menlo's wider advance hid on one row.
5. **The rustkit-engine suite cannot be run on this machine while the other lanes build:** every failure in two attempts was its GPU test guard timing out after 120s, not an assertion. CI is the only full run of it today.

**Not done:**
- L0 (queue item 5), `::first-letter`, `unicode-range` in the web-font registry, inset shadows, a real shadow blur, select sizing under `box-sizing`.
- A scoring board after #411/#412/#417 (load).

**Next:** (1) anything CI, R1 or R2 ask on #417; an engine test that pins the old default face would show in CI first. (2) Glyph fallback through Core Text's cascade (`CTFontCreateForString` on the primary face), measure and paint together (Found 3). (3) L0 under the reading in decision 1. (4) `unicode-range`. (5) A quiet board.

**Decisions for Pete:**
1. **L0: build it next session under this reading, unless you say otherwise.** The three open points from the 14:25 digest have answers inside the design as written, and I verified the design text against them: (a) grid has no column width when it takes the height contribution, so for in-slice items the block contribution is taken after column sizing, by laying the item's subtree out at its column width (the design's own rule: "the same measurement final layout uses"); (b) the fixture is the one section 6 describes (a grid of auto rows, each item a nested flex row that wraps and holds a button), and the two named pages stay as descriptions of the failure, not as fixtures; (c) "Phase 9.5 delta is 0" is reported as the raw difference between the fragment and the flowed height for each in-slice item (expected exactly 0), with the border included in the fragment. If Prometheus should rule on any of these first, say so and L0 waits.
2. **Is a LOADS point for a nearly blank page worth keeping honest this way?** instagram lost its point by becoming more correct. No rule change is proposed; this is only so the 26 is not read as a regression.
3. **Glyph fallback (Found 3) before or after L0?** It costs 7 geometry boxes on `about` today and affects every page with symbols in a face that lacks them. Recommendation: before L0, it is a day at most and it removes a regression #417 exposes.
