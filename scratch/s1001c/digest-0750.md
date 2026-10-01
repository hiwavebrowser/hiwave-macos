
## 2026-10-01 08:25

**Points: 28/60 -> 28/60 (no scoring run this session).** The number is this morning's quiet board on develop 7ae0e68 (05:29): loads 18, readable 7, looks-right 3; scorable 27/57. Machine load was 13 to 19 all session, so the Chrome oracle was not usable; live-site evidence below is RustKit frames only.

**Queue status:** item 1 (#396) and item 2 (#398) are both MERGED. Item 3 is open as #401. `::first-letter` is not started.

**PRs opened:**
- **#402 `atlas/rs-font-resolve-test-flake` @ 5db7680.** Test code only. Two `rustkit-layout` font-cache tests raced other tests' web-font installs: develop f16ad4e failed 579/580 on 4 parallel runs out of 4 here. With the fix, 580/580 on 8 consecutive runs. Every CI check passed by 08:21; no review yet.
- **Branch pushed, NO PR yet: `atlas/rs-logical-borders` @ 04ebb59** (base f16ad4e). The flow-relative border properties (`border-inline-*`, `border-block-*`) map to the physical sides; a two-value logical shorthand is split at top-level whitespace, so a colour with spaces is one value. One engine test, which fails on develop's code and passes with the change. **Its campaign receipt did not fit the session** (the release build was still running at the cap, 18 minutes in at load 14 to 23), so no PR was opened: R2's gate 5 needs the receipt in the body. The PR body is drafted (`scratch/s1001c/fill_logical.py`). Not verified on facebook itself yet.
- **#401 `atlas/rs-elliptical-corners` @ 740efcb** (base f16ad4e; commits d66616b, 740efcb). Queue item 3. **08:02: every CI check passed at 740efcb (unit-suites, pr-swarm, f1-test-compile) and R1 is CLEAR; no R2 stamp yet.**
  - **What was wrong:** one scalar per corner from the parser to the rasterisers. `50px / 25px` painted its horizontal half as a circle, a two-value longhand was dropped, a percentage used the width on both axes, and each rasteriser cut every radius to half the shorter side instead of the spec's overlap reduction.
  - **Fix:** a horizontal and a vertical radius per corner through computed style, used values, the display list, and the fill, border, gradient and clip rasterisers. Percentages resolve per axis. One overlap factor for all eight radii (CSS Backgrounds 3 §5.5), applied in layout. The overflow clip's inner radius is inset per axis by the border beside it. The flow-relative longhands (`border-start-start-radius` ...) now parse.
  - **Tests:** 5 engine pins **fail on develop f16ad4e** and pass on the branch; plus 2 parser tests, 2 layout tests, 9 renderer tests (all device-free). The test that pinned the wrong behaviour (`an_elliptical_radius_takes_the_horizontal_half`) is replaced. rustkit-renderer 100/100, rustkit-layout 581/582 (the one failure is on develop too, see Found 2), the 21 engine radius tests pass. Full engine suite not run locally; CI is that evidence.
  - **Campaign vs develop f16ad4e:** `rounded-corners` **1.3221% -> 0.4354%**. Every other case has the same `diff_pct` (all 25/26, builtins 5/5, micro 12/13 identical). Gate B paint rises on three cases (rounded-corners 0.97677 -> 0.98413, card-grid 0.82508 -> 0.82521, about 0.93939 -> 0.93940) and falls on none.
  - **Circles are unchanged to the bit**, which is why the receipt is that clean: equal radii fit to exactly half the side, and the edge distance has an exact circle path.

**Real-site A/B (frames only, develop/fix/develop/fix):**
- **facebook 0.11%:** the login inputs and buttons are now rounded. On develop the email and password boxes are square in the display list; on the branch they have 16px corners (buttons 22px). The page declared those radii in a form develop dropped.
- x 0.03% (four corners of one box), shopify 0.02% (the hero pill). lyft, google and apple: no change on the captures that loaded. Every change found is a corner; nothing moved.
- No points claimed. 0.1% of a frame cannot flip LOOKS RIGHT on any site today. The next quiet daily board is the measurement.

**Not done, plainly:**
- **Shadow corners and rounded clipping of images and glyphs.** The queue item says "clip, border and shadow". The shadow carries no corner radius at all today, and textured quads under a rounded `overflow: hidden` get only the rectangular clip. Both are new work and are not in #401. Proposed as the next PR.
- `vw`/`vh` radii still resolve to 0. Logical radii assume an ltr horizontal box.
- `::first-letter`: not started.
- Select sizing under `box-sizing` (follow-up to #396): not started.
- The full 20-site board A/B the PLAN asks for after a paint change: not taken (load).

**Found:**
1. **A second bug in the old corner rasteriser**, fixed in #401's second commit: a right or bottom corner with a fractional radius (`25%` of 150px) was sampled half a pixel off the grid and painted a notch down the side. It is visible in develop's `rounded-corners` capture too. Found by looking at the first build's frame, not by a test; there is a test now.
2. **A flaky test on develop:** `rustkit-layout` `text::font_resolve_tests` failed one shape-count assertion whenever the suite ran in parallel here (passes alone). The web-font generation is process-wide and several tests install a set. Fixed in #402.
3. **facebook's login inputs paint only their top and bottom borders** on develop. The engine had no arms for the flow-relative border properties (`border-inline-*`, `border-block-*`), which StyleX and Tailwind v4 emit. That is the logical-borders branch above.
4. The shared cargo target bit again in a new way: a `cargo test` in the develop worktree linked the fix branch's rlibs and failed to compile until every source was touched. `ctest.py --touch` whenever the arm changes.

**Next:** (1) anything R1/R2 ask on #401, #402 and the logical-borders change. (2) The second half of item 3: corner radius on `BoxShadow`, and the rounded clip applied to textured quads. (3) `::first-letter`. (4) Select sizing under `box-sizing`. (5) A quiet board once these land.

**Decisions for Pete:**
1. **Item 3 was split, as recommended last night.** #401 is the representation plus fill, border, gradient and clip. Shadow corners and rounded image clips are a second PR. Say so if you want them in one.
2. **Order after that: rounded image clips before `::first-letter`?** Recommendation: yes. Avatars and card images under a rounded `overflow: hidden` are on most of the board; a drop cap is on few pages.
3. **Build contention, third time.** Three release builds this session took 14, 16 and 17 minutes (6 when quiet), and the Chrome oracle was unusable again. A protected slot for the board, or staggered lanes, is still open.
