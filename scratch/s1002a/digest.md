
## 2026-10-02 02:10

**Points: 26/60 -> 26/60 (no scoring run this session).** The number is the 2026-10-01 14:54 quiet board on develop 2b764be: loads 16, readable 7, looks-right 3; scorable 25/57. Load was 4 at the start and 11 to 19 for the rest (three release builds and two campaign arms of mine, plus the other lanes), so no Chrome oracle run was taken. Live-site evidence is RustKit frames A/B, scored against the 14:54 board's stored Chrome frames.

**The 15:10 question (28 -> 26, instagram and microsoft)** is answered in the 2026-10-01 17:28 section and was not re-run.

**Queue status:** items 1 to 4 are MERGED, and **#426 MERGED overnight** (develop f60d1d6). Item 5 (L0) is not built; see decision 1. `::first-letter` is not started.

**PR opened (Prometheus R1 + Cursor R2; not mine to merge):**
- **#429, DRAFT** `atlas/rs-button-ua-box` @ 19ec405 (base f60d1d6; commits bfe27b4, 19ec405): **a push button has Chrome's default box, line and face.** `<button>` and `<input type=button|submit|reset>` get `padding: 1px 6px`, a 2px border, `box-sizing: border-box` and `line-height: normal` from the engine's default style, and layout composes the button from them instead of standing in label + 24 by 19.
  - **Against Chrome 148:** a bare "Go" was 41.79 x 19 and is 33.79 x 21 (Chrome 33.80 x 21); a `padding: 0; border: 0` reset was 41.79 x 19 and is 17.79 x 15 (17.80 x 15); `line-height: 30px` was 19 tall and is 36 (36). Boxes more than 0.5px off: 15 of 15 -> 2 of 15 on one page, 12 of 12 -> 7 of 12 on a second (all seven are one `box-sizing: content-box` button and the boxes it moves).
  - **Tests:** five new and one replaced; all five **fail on develop's code** and pass on the branch. rustkit-layout 602/602. CI's full engine suite (`unit-suites`) is green at 19ec405.
  - **Campaign vs develop f60d1d6:** all 25/26, builtins 5/5, micro 12/13 identical. The mover is `form-controls`, 3.2388 -> 2.8713%, and its geometry gate goes 43 -> 32 failures with none new.
  - **Real sites, 17 captured:** 8 pixel-identical on all four captures, shopify and facebook identical on the captures that finished, 4 within their own variance (linkedin, bing, yahoo, netflix), walmart 0.26% and squarespace 1.94% across arms with no change against Chrome, and **weather 0.83%, which is a regression in paint** (Found 1). github, cnn and instagram were not captured on either arm (30 s limit, two attempts). No check changes.
  - **Why it is a draft:** Found 1. It must not merge before the `background` fix.

**Branch pushed, NO PR yet: `atlas/rs-background-shorthand-reset` @ a45dcb2** (base 363c3de). The fix for Found 1, with one engine test that passes on the branch. It has no fail-first run on develop, no campaign receipt and no real-site A/B. It changes what every `background:` declaration does on every page, so it needs the A/B on all sites before a PR.

**Found:**
1. **The `background` shorthand never clears a colour.** It sets `background-color` only when the whole value is a colour. So `background: none`, `background: 0 0` (what minifiers write for `none`), an image alone, `initial` and `unset` over an earlier colour all leave that colour painted; Chrome clears it. This is on develop and predates this session. #429 exposed it: once a button's grey face comes through the cascade, a site that removes the face with `background: none` keeps it. weather.com's "More" navigation button shows a grey pill on the fix arm; Chrome shows none. On a test page, 7 of 12 ways to clear a button's background fail (`scratch/s1002a/bgreset.html`). The fix is written (the branch above). Two of the seven are not covered by it: `all: unset` is not applied at all, and a four-digit hex colour (`#0000`) is not parsed.
2. **With the button fixed, L0's own fixture matches Chrome without L0.** 25 of 25 boxes were off on develop; 23 of 25 match on #429's binary. The two left are the percentage-height item, which the design puts outside the slice. The wrapped-text rows were off only because the button took 8px from the text beside it.
3. **A control ignores `box-sizing: content-box`.** An explicit width and height are taken as the border box (Chrome: 100 x 40 becomes 116 x 46 on a button). Measured, not fixed.
4. **Chrome splits an odd leading 7 above and 8 below; RustKit splits it 7.5 and 7.5.** It shows as 0.6px in y on a line whose baseline comes from a `line-height: 30px` button. Measured, not fixed.
5. **A release build was 14 to 15 minutes** at load 11 to 13, three times. The first fix binary needed a second build after the fixture showed four more things (the frame over a reset button's label, an empty button's baseline, an `<input>` button's line, the face of a button with children).

**Not done:**
- L0 itself, `unicode-range`, `::first-letter`, inset shadows, a real shadow blur, the emoji-presentation rule.
- The same default-box rule for text fields and selects (they keep their calibrated sizes).
- A scoring board.

**Next:** (1) receipts for `atlas/rs-background-shorthand-reset` (fail-first, both arms, all-site A/B, campaign), then its PR. (2) When it merges: merge develop into #429, rebuild, confirm weather's button, re-receipt, mark it ready. (3) Text fields and selects. (4) `unicode-range` or L0, per decision 1.

**Decisions for Pete:**
1. **L0 next, or `unicode-range`?** L0's acceptance is its fixture matching Chrome within 0.5px, and after #426 and #429 the fixture already does, except for the item the design excludes (Found 2). So L0 would now be a refactor with nothing on its fixture to fix; `new_tab` still has 6 geometry failures and `about` 60, and I have not checked how many of those L0 would take. `unicode-range` has three board sites behind it (x, shopify and weather draw none of their text in their own font). Recommendation: `unicode-range` first; and before L0 is built, Prometheus names a page that still fails for the reason L0 fixes. If you want the queue order kept, say so.
2. **The `background` fix changes every page that uses the shorthand.** It is the spec's behaviour and Chrome's, and I expect it to move live sites in both directions (a colour that survived by accident will go; a colour beside an image will appear). Recommendation: let it go through normal review with the all-site A/B in the body. Say so if you want to see the A/B before the PR opens.
