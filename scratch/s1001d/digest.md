## 2026-10-01 11:20

**Points: 28/60 -> 28/60 (no scoring run this session).** The number is this morning's quiet board on develop 7ae0e68 (05:29): loads 18, readable 7, looks-right 3; scorable 27/57. Machine load was 13 to 30 from 09:15 on, so the Chrome oracle was not usable for a board; live-site evidence below is RustKit frames only.

**Queue status:** items 1, 2 and 3 are MERGED (#396, #398, #401), and so are #402 and #403 from the last session. The second half of item 3 (shadow corners, rounded clipping of images and glyphs) was opened as #407 and **MERGED this session** (develop c6b4841). `::first-letter` is not started.

**PRs opened (Prometheus R1 + Cursor R2; not mine to merge):**
- **#407 `atlas/rs-rounded-shadow-clip` @ fc64b0e** (base 473a047; commits 2f3ebbe, fc64b0e). Every CI check passed at fc64b0e by 11:01; **merged by 11:13.**
  - **Images were drawn without blending.** The image batch used the blit pipeline, whose blend is REPLACE, so a transparent texel overwrote the pixel under it. **Every PNG with a transparent surround painted as a logo in a black box.** Fixed with an image pipeline that composites source-over. Found because antialiasing a rounded image clip needs it; it is the larger fix of the three for real sites.
  - **Box shadows follow the box's corners.** The shadow command now carries the box's radii. An outer shadow is the box's shape moved and spread (radii grown by the spread, per the spec's rule for small radii), with the box's own curve cut out. A square box takes the old code path, rect for rect.
  - **A rounded `overflow: hidden` clips images, background tiles and glyphs.** An avatar in a `border-radius: 50%` box painted as the whole square photo; it is a disc now. Text between the arcs of a pill is still one quad per glyph.
  - **Tests:** two engine tests **fail on develop 473a047** and pass on the branch (a display-list pin; a headless frame test that reads seven pixels). Plus 1 layout test and 10 device-free renderer tests. rustkit-renderer 110/110, rustkit-layout 583/583. Full engine suite not run locally; CI ran it.
  - **Campaign vs develop 473a047:** all 25/26 identical, builtins 5/5, micro 13/13. **One case is worse:** `card-grid` 1.3068 -> 1.3074 (about six pixels; its blurred card shadows now have rounded corners). Gate B paint rises on five cases (card-grid 0.82521 -> 0.82911, sticky-scroll, css-selectors, flex-positioning, form-elements) and falls on none. Said so in the PR body.
  - **Fixture vs Chrome 148** (12 boxes, `scratch/s1001d/shadow-clip.html`): 6.873% -> 2.220% of the region differs. The transparent PNG over blue goes 9,384 differing pixels -> 0; the image in a circular clip 3,256 -> 204.

- **#409 `atlas/rs-img-border-radius` @ a739e7d** (one commit on top of #407's head; opened 11:19, CI not finished when the session stopped, no review yet). `border-radius` on the `<img>` itself now rounds the image; it only rounded the image's background. That is how most avatars are written (`img { border-radius: 50% }`, no wrapper).
  - **Test:** one engine pin, which **fails on develop c6b4841** (1 failed, 19 passed) and passes on the branch. rustkit-layout 583/583.
  - **Campaign vs develop's engine:** all 26/26, builtins 5/5, micro 13/13 identical; ratchet output identical. The develop arm is #407's head binary: develop c6b4841 adds only #405 and #406 to it, which change nothing under `crates/`.
  - **Fixture vs Chrome:** that box 852 differing pixels -> 98; the whole region 2.220% -> 2.047%. No live-site A/B for this one (cap).

**Real-site A/B (frames only, develop/fix/develop/fix, 14 sites):**
- **microsoft 0.17%:** the header wordmark was a dark block; it reads "Microsoft" now. **bing 0.04%:** a black square beside "Copilot" is gone, and the search box's shadow is rounded. **wikipedia 0.02%:** the padlock icon loses its dark backing. lyft 0.08% (one 784x1 line), walmart 0.02% (one icon). x and yahoo: no change.
- google showed 3.76% between the arms. **That is not the change:** Google serves two variants of its page per request. Six more captures in mixed order gave each binary both variants; within one variant the arms differ by under 0.01%.
- facebook, apple, github and netflix failed to capture on both arms (load); shopify's fix capture failed. linkedin's change is inside its own variance.
- No points claimed. The next quiet daily board after #407 lands is the measurement.

**Not done, plainly:**
- **Inset shadows.** They do not use the radii, and they are wrong on develop in a bigger way: an unblurred inset shadow fills the whole inner rect instead of the band at the edge. Not touched.
- **A real blur.** The shadow blur is still a stack of expanding flat layers. Its shape is right now; its falloff is not (the blurred pill is still 5,594 pixels off Chrome in the fixture, the largest box left).
- `::first-letter`: not started. Mapped only (PLAN note).
- Select sizing under `box-sizing` (follow-up to #396): not started.
- The full 20-site scoring board the PLAN asks for after a paint change: not taken (load).

**Found:**
1. **The black-box bug hid for the whole campaign** because no campaign case and no fixture has a transparent image over a non-black background. The new engine frame test is the first pixel test of an `<img>`.
2. **A consistent A/B difference on a live site can be the site.** Google alternated variants in step with the capture order, which looked exactly like a regression. `scratch/s1001d/variant.py` re-captures in mixed order and settles it in about a minute.
3. Rounded clips under a rotation or skew are still rectangular, and gradient text bypasses the clip stack entirely (both pre-existing, both noted in #407).

**Next:** (1) anything R1/R2 ask on #409. (2) Inset shadows with `rounded_difference_pieces`. (3) `::first-letter`. (4) A quiet board, now that #407 is on develop.

**Decisions for Pete:**
1. **Shadow blur: do a real one before `::first-letter`?** The layered approximation is now the biggest visible shadow error, and card shadows are on most of the board. Recommendation: inset shadows first (small, and they are plainly wrong), then a proper blur, then the drop cap.
2. **Build contention, fourth time.** A release build took 22 minutes (6 quiet) and a campaign arm 12 (under 3 quiet); Chrome's oracle was unusable again and four sites could not be captured at all. A protected slot for this lane's builds and the board is still open.
