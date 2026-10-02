
## 2026-10-02 08:20

**Points: 26/60 -> 26/60 (no scoring run this session).** The number is the 05:51 daily quiet board on develop 97393a7: loads 16, readable 7, looks-right 3; scorable 25/57. The machine was quiet from 06:05 until some time before 07:07 (load 2 to 5: two release builds at 6 minutes each, and an all-site A/B in which every one of the 20 sites was captured four times) and at 11 to 18 after that (the other lanes), so no Chrome oracle run was taken. Live-site evidence is RustKit frames A/B, and for the three sites that moved, the oracle's own diff against the Chrome frames the 2026-10-01 14:54 quiet board stored.

**The 15:10 question (28 -> 26, instagram and microsoft)** is answered in the 2026-10-01 17:28 section and was not re-run.

**Queue status:** items 1 to 4 are MERGED. Item 5 (L0) is not built and waits on decision 1 of the 02:10 digest. `::first-letter` is not started.

**PR opened (Prometheus R1 + Cursor R2; not mine to merge):**
- **#435** `atlas/rs-hex-short-alpha` @ 71bfac0 (base develop 97393a7; one commit): **a four-digit hex colour is parsed (`#rgba`).** The colour parser knew three, six and eight digits, so a declaration with four was dropped and the colour under it stayed. Minifiers write `transparent` as `#0000`.
  - **Against Chrome 148** (fixture `scratch/s1002c/hex4.html`, the colour painted in each of 13 boxes): 7 of 13 wrong on develop, 0 of 13 on the fix.
  - **Tests:** two in rustkit-css, one in the engine; the engine test **fails on develop's code** (`background-color: #0000` over red stays red) and passes on the branch. rustkit-css 49/49.
  - **Campaign vs develop 97393a7:** all 26/26, builtins 5/5, micro 13/13 identical; ratchet identical line for line.
  - **Real sites, 17 captured:** **github moves across the whole frame: 75.06% -> 16.24% against the stored Chrome frames** (Found 2). weather 0.01% and shopify 0.05% across arms (30.53 -> 30.54% and 19.906 -> 19.905% against Chrome). Ten sites pixel-identical; netflix, google, walmart and linkedin differ within one arm as much as across (each serves more than one page). **Not verified: squarespace, facebook, cnn** (30 s limit on both arms, two attempts, load 11 to 14).
  - **At 08:17:** CI all green at 71bfac0 (`unit-suites`, the full engine suite, included), R1 CLEAR; no R2 stamp yet; not merged.

**#434 MERGED overnight** (develop 97393a7). Nothing was asked on it.

**#429 (the push button's default box) is still a DRAFT, and now waits on #435.** I merged develop 97393a7 into its branch (eda63a8; rustkit-layout 605/605 and the engine's five button tests pass on the merged tree) and pushed the merge. On the rebuilt binary the fixture is right for what #433 fixed: `background: none`, `0 0`, `unset` and `initial` show no face. The all-site A/B (20 of 20 sites captured on all four runs) still shows the two regressions: weather's "More" pill, and squarespace's three navigation buttons, which get a grey face over the hero. Both come from the same dropped declaration, `background-color:#0000` (Found 1). github moves 0.16% on #429 and was not looked at.

**Found:**
1. **`#0000` was not a colour.** Tailwind's preflight resets every button and field with `background-color:#0000` (weather.com's sheet has `button,input,select,optgroup,textarea{background-color:#0000}`), and squarespace's buttons carry `.cta--inline{appearance:none;background-color:#0000}`. This is what kept #429 in draft after #433; the 02:10 digest had it as one of two leftovers on a test page and I did not then check which form the live sites use. They use this one.
2. **github's header, hero and e-mail field were painted wrong for this reason alone.** On develop the header is a black band and the field is filled white; on the fix the header is clear over the hero's gradient, as in Chrome. 75.06% -> 16.24% is against Chrome frames stored a day earlier, so it is a size, not a board score; LOOKS RIGHT needs 15%. What the remaining 16% is has not been looked at.
3. **The parser also took a sign inside a hex colour** (`#+f+f+f` painted as a colour; Chrome drops it). Fixed in #435.
4. **The colour parser reads `rgb()` and `hsl()` by commas only** (read in the code, not measured): `rgb(0 0 0 / 50%)`, percentages, `hwb()`, `oklch()`, `color-mix()` and the system colours would each drop the declaration the same way `#0000` did. Decision 2.
5. **A `cargo test` in a second worktree against the shared target ran the first worktree's test binary** (four tests where the branch has five; 19 s). Touching the crate sources gave the real run. The note about this from 2026-10-01 holds for test builds between branches, not only for release arms.
6. **A release build was 6 minutes at load 3 to 5 and 15 minutes at load 12 to 18.** A campaign arm (micro, builtins, all, ratchet) was 9 minutes at load 11 to 14.

**Not done:**
- #429's un-draft (it needs #435 on develop first).
- #435's A/B rows for squarespace, facebook and cnn.
- Routing image fetches through the loader, the background-image fetch PR, length positions, SVG backgrounds.
- L0, `unicode-range`, `::first-letter`, text fields and selects' default box, inset shadows, a real shadow blur, the emoji-presentation rule.
- A scoring board.

**Next:** (1) When #435 is on develop: merge develop into #429, rebuild, A/B weather and squarespace (the pill and the three nav faces must be gone), re-receipt, mark ready. (2) A scoring board on a quiet machine once #435 lands: github's LOOKS RIGHT is the check to watch. (3) What is left of github's 16% (`scratch/s1001c/where_diff.py` against Chrome's frame). (4) The image loader routing, then the fetch PR (05:25 digest, decisions 1 and 2).

**Decisions for Pete:**
1. **Still open from 05:25: background images next, with image fetches routed through the loader first?** Nothing changed; my recommendation is still yes to both. I did not start it this session because #429 was first in the plan and led to #435.
2. **Count the declarations the engine drops on the 20 board sites before picking the next fix?** github's 75% was one unparsed colour form, found by accident while un-drafting a button PR. A counter at the point where a declaration's value fails to parse (property, and the value's shape: function name or unit), dumped per site, would rank what is still dropped: the modern colour syntaxes of Found 4 are my first guess, but it is a guess. One session, no engine behaviour change. Recommendation: yes, right after #429 is ready, and ahead of `unicode-range` and L0.
3. **#435 also carries a low-severity input-hardening change in the colour parser; details withheld here.** The diff is small and public. Nothing to decide unless you want it split into its own PR.
