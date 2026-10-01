# 2026-08-16 — slice-0 verified end-to-end; the break-word alias was order-fragile

## What this session did
The soft-wrap slice-0 lane (n24: "finish as one unit") was found ALREADY FINISHED
on `atlas/softwrap-slice0` by the two orphan sessions between n24 and tonight
(`2aadc9b` Aug 14, `1600cd2` Aug 16 02:12) — plus one more uncommitted layer on
the shared checkout (the line-count ceiling assertion), banked verbatim as
`4fdc96f` before analysis. Fifth mid-flight recovery in the chain (n18, n22,
n24's find, 1600cd2's find, tonight's find).

Tonight verified the whole stack, measured it honestly, found one real
regression, root-caused and fixed it, and opened the PR.

## Verification (all green)
- `rustkit-text`: 68 unit + 6 doc tests.
- `rustkit-layout`: 260 unit + 1 probe.
- `rustkit-engine` `test_word_break_and_line_break_reach_the_line_breaker`:
  passes WITH the new line-count ceiling (4–12 lines for a 34-char word in a
  40px box) — the assertion that can actually fail on one-grapheme-per-line,
  closing n24's "test cannot fail for the right reason" gap.

## Measurement (fresh parity-capture both runs; freshness guard from `1600cd2` active)
- Campaign pixel board: **26/26, avg 6.6% — zero movement** either direction.
  Slice-0 is invisible to the campaign suite (no case uses break-all/anywhere).
- WPT Tier-1 vs committed 7/25 (28.0%):
  - First run (slice-0 stack only): **8/25** — overflow-wrap-001/002 flip to
    PASS at 0.0 diff, but `break-word-overflow-wrap-interactions` REGRESSED
    from PASS to FAIL (0.17%, 816 px).
  - The master PASS was coincidental — two-wrongs-make-a-right: word-break
    never reached the line breaker, so test and ref both rendered identically
    wrong. Slice-0 made the defect visible, it did not create it.
- Final run (with the fix below): **9/25 (36%)** — interactions back to PASS at
  0.0. Net vs committed basis: **+2 passes, zero regressions.**

## The regression's root cause (source-traced AND measured)
Pixel-diff localization: the 816 px sat entirely in box 3 of the test
(`word-break: break-word; overflow-wrap: normal`) — boxes 1/2 were pixel-exact
vs ref. Minimal repro (100px div, monospace, single long word):
- `word-break: break-word; overflow-wrap: normal` → ink band x8–154, ONE line
  (never breaks, overflows).
- `overflow-wrap: anywhere` → x8–110, wraps correctly.

Cause: the engine's `word-break` parse arm implemented the legacy `break-word`
value by writing `style.overflow_wrap = BreakWord` and computing word-break as
Normal. Declaration order kills that: a later `overflow-wrap: normal` in the
same style attribute overwrote the alias's only effect. This is exactly the
interaction the WPT test exists to catch (Mozilla bug 1296042).

Fix (`a70453e`): `break-word` computes as `WordBreak::BreakWord`, its own
value; `overflow_wrap` untouched. rustkit-layout already honors BreakWord in
`may_break_mid_word` (the emergency fitted-grapheme arm), so no layout change.
Repro now renders both variants identically; interactions PASS 0.0.

## Named residuals (ledger, not chased tonight)
- The emergency fitted-prefix loop in `wrap_segment` re-shapes every grapheme
  prefix: O(n²) per overflowing word. Correctness first; optimize only if a
  profile ever names it.
- 12 of the 16 remaining Tier-1 fails stay attributed `@font-face
  unimplemented`; most now sit at tiny diffs (0.07–2.7%) — slice-0 moved
  nearly the whole family closer without flipping them.
- n24's structural note stands: the @font-face lane exists on `develop`
  (PRs #124–#133) while this board measures `master`.

## Instruments
- The stale-binary guard (`scripts/wpt_tier1.py`, from `1600cd2`) ran live on
  every board tonight — n24's ask is landed and exercised.
- `parity_test.py` still builds nothing itself; both boards tonight were run
  only after an explicit `cargo build --release -p parity-capture`.
