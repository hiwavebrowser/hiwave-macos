
## 2026-10-01 23:10

**Points: 26/60 -> 26/60 (no scoring run this session).** The number is today's 14:54 quiet board on develop 2b764be: loads 16, readable 7, looks-right 3; scorable 25/57. Load was 4 to 23 during the session (three release builds and three campaign arms of mine, plus the other lanes), so no Chrome oracle run was taken. Live-site evidence is RustKit frames A/B, scored against the 14:54 board's stored Chrome frames.

**The 15:10 question (28 -> 26, instagram and microsoft)** is answered in the 17:28 section and was not re-run.

**Queue status:** items 1 to 4 are MERGED. **Item 5 (L0) is not built.** I wrote its fixture, measured it in Chrome, and it exposed two bugs that sit under it; one is fixed in #426, the other is Found 1. `::first-letter` is not started.

**PRs opened (Prometheus R1 + Cursor R2; not mine to merge; #425 merged, #426 open):**
- **#425** `atlas/rs-intrinsic-letter-spacing` @ 848c872 (develop f657cf2 merged in; the fix is 39589a4 from last session, plus a test commit): **the intrinsic width of text includes `letter-spacing` and `word-spacing`.** `new_tab`'s "HIWAVE" logo was 165.66px wide and 24.6px off centre; it is 213.66 and 0.58px off. Chrome's is 214.80, so the logo's four rows still fail the 0.5px gate (Found 3).
  - **Test:** one, now run four ways (inline-block and flex item, each through both layout entry points). It **fails on develop's code** (184.05 against 184.05 + 48). rustkit-layout 595/595.
  - **Campaign vs develop f657cf2:** all 25/26, builtins 4/5, micro 13/13 identical. The mover is `new_tab`, 1.5003 -> 1.2713. Ratchet identical except `new_tab`'s paint score (0.9452 -> 0.9469).
  - **Real sites, 16 captured:** microsoft (0.74% of pixels) and shopify (1.04%) differ across arms; against Chrome 54.38 -> 54.37% and 19.91 -> 19.91%. The rest are identical or within their own variance. No check changes.
  - **MERGED at 22:54** (develop 46f6d5f), after CI green, R1 DESIGN CLEAR and R2-STAMP PASS at 848c872.
- **#426** `atlas/rs-grid-flex-item-used-height` @ 026fcc5 (base f657cf2): **a grid item that is a flex container fills its row and aligns its items in it.** On develop it stayed as tall as its content unless a row repair happened to fire (a 38px card in a 100px row), and its `align-items: center` children were never re-centred.
  - **Tests:** four; three **fail with the fix switched off** ("fills the 100px row, got 20"), the fourth is the `align-self: start` guard. rustkit-layout 598/598.
  - **Reduced page against Chrome 148:** 6 of 11 boxes off on develop, 0 of 11 on the fix.
  - **Campaign vs develop f657cf2:** all 25/26, builtins 4/5, micro 13/13 identical; `new_tab` 1.5003 -> 1.3309. **Geometry gate: `new_tab` 17 -> 6 failures, `about` 66 -> 60**, and no box fails that passed on develop.
  - **Real sites, 18 captured: apple 54.81% -> 41.65% against Chrome** (still a fail). Its hero panel was cut off at 290px with stray navigation labels under it; it is 580px with the product image in it. Every other site is identical across arms or within its own variance. No check changes.
  - **At 23:05:** CI running (4 green, 6 pending, none failed), MERGEABLE against develop with #425 in it, no review yet. Its receipt is against f657cf2, so it does not include #425; the two touch different parts of `grid.rs`.

**Found:**
1. **Every unstyled `<button>` is 8px too wide and 2px too short, and ignores `line-height`.** Chrome 148's button is its label plus 16 wide (padding 1px 6px, border 2px) and its line plus 6 tall: "Go" is 33.80 x 21 in Chrome and 41.79 x 19 here; with `line-height: 30px` Chrome is 36 tall and RustKit 19. The engine's default style gives a button no padding or border, and layout stands in a fixed figure (label + 24, 19px) that was measured on a fixture whose reset sets `padding: 0`. A page that resets button padding to 0, as Tailwind's base styles do, gets label + 24 where Chrome gives label + 0. Measured, not fixed: `scratch/s1001h/l0/btn.html`.
2. **The L0 fixture cannot pass until Found 1 is fixed.** The fixture is the one the design's section 6 describes (auto rows, nested flex rows with wrapped text and a button). With #426 its only remaining causes are the button box (the 8px takes a line's worth of width from the wrapped text, so row 1 is 98px against Chrome's 78) and one out-of-slice item (a percentage height in an auto row: 36 against 49.75). 25 of 25 boxes are off on both arms for those two reasons.
3. **The logo's last 1.14px is the six letters, not the spacing** (165.66 measured against Chrome's 166.80 for "HIWAVE" at 48px). Not looked into.
4. **A release build took 40 minutes** at load 14 to 23 (6.5 minutes at load 4). Two campaign captures and seven site captures failed on load alone and were re-run or reported as uncaptured.

**Not done:**
- L0 itself (the constraint and fragment types, the two call sites, the four-number differential).
- `unicode-range`, `::first-letter`, inset shadows, a real shadow blur, select sizing, the emoji-presentation rule.
- A scoring board. github, squarespace and cnn were not captured for #425 (load); instagram has no complete develop/fix comparison for #426 and one identical pair for #425.

**Next:** (1) anything CI, R1 or R2 ask on #426. (2) The button's default box (Found 1), since L0's own fixture waits on it. (3) L0. (4) `unicode-range`.

**Decisions for Pete:**
1. **Button default box before L0?** L0's acceptance is "item boxes on the fixture match Chrome within 0.5px", and the fixture has a button by design. Recommendation: fix the button first (one PR: default padding and border in the engine's default style, the control's size composed from them, `line-height` honoured), then L0. The alternative is to give the fixture's buttons author padding so the bug does not show; that passes L0 and leaves every unstyled button on real pages wrong.
2. **`unicode-range` before L0** is still open from the 20:00 digest (x, shopify and weather draw none of their text in their own font). My order if you say nothing: button box, L0, then `unicode-range`.
3. **#426 costs one extra flex layout for each flex card shorter than its grid row.** Capture times show no site consistently slower, but facebook sits at 23 to 30 s on both arms against the 30 s limit, and I could not separate the arms from machine load. If the cascade lane sees layout time move on a grid-heavy page after it merges, this is the place to look.
