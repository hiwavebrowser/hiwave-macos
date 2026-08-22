# Slice-0 soft-wrap is dead TWO layers deep — the orphan fixes the top one

Night 24 (macOS seat), 2026-08-12. Base: `hiwave-macos` master `44389f1`.

## What was found in the tree

The seat was left mid-flight on an **unpushed** branch `atlas/ch-units` (commit
`448ba2e`) with a further ~190-line uncommitted diff on top. This is the **third
orphan in the chain** — n18, n22, and now n23 all died pre-commit and left work
only on disk.

Two separable units were recovered:

1. **`atlas/ch-units` (`448ba2e`)** — the `ch` unit resolved against the
   element's own font. Already committed by n23, **but it does not compile**:
   its own new test calls `measure_text_advanced` with `&["monospace".to_string()]`
   while master's signature takes `font_family: &str`. The fix was sitting in the
   uncommitted diff, so the pushed commit alone is red.
2. **The soft-wrap (slice-0) WIP** — the lane PLAN/n19/n20/n21 all named as the
   Tier-1 target.

Both are now banked verbatim on `atlas/softwrap-slice0` (`8c1090b`) and pushed,
**before any edit**, so a fourth death cannot lose them.

## What the WIP does (and it is correct)

- `rustkit-css`: new `OverflowWrap` enum, `ComputedStyle.overflow_wrap` field,
  inheritance.
- `rustkit-engine`: parse arms for `word-break`, `overflow-wrap` / `word-wrap`,
  and `line-break: anywhere`. **`word-break` was a consumer with no producer** —
  the enum, the ComputedStyle field, the CSS→LineBreaker conversion and the
  break-all algorithm all existed and were unit-tested, but no declaration ever
  assigned the field, so it was permanently `Normal`. `overflow-wrap` had no
  computed representation at all.
- `rustkit-layout`: plumbs `overflow_wrap` through `wrap_text` /
  `wrap_text_with_first_line` and stops hardcoding
  `LineBreaker::new(lb_word_break, OverflowWrap::Normal)`.

That is the **tenth parsed-but-dead behavior** of the campaign.

## The finding: the layer below is dead too

Un-hardcoding the flag opens a gate onto machinery that is itself decoration.

`rustkit-text/src/line_break.rs`, `LineBreaker::break_opportunities` contains,
verbatim:

```rust
let _overflow_wrap = self.overflow_wrap; // Reserved for future use
```

It returns only UAX-14 opportunities. Its `word_break == KeepAll` branch and its
`else` branch **both** return `BreakKind::Allowed`, so `word-break: break-all`
is inert here as well. `LineBreaker::emergency_breaks()` — the function that
would produce grapheme-boundary opportunities — has **exactly one caller in the
tree: its own unit test** (the standing check from n20/n21: *a type with a test
and no non-test caller is decoration*). That is the **eleventh**.

Consequence, traced through the real path for a long unbreakable word in a
narrow box:

1. `TextShaper::find_line_break` asks `breaker.break_offsets(text)`, which
   ignores `overflow_wrap` → no interior opportunity fits → returns `0`.
2. `TextShaper::wrap_segment` falls into its `break_offset == 0` arm.
3. `allows_emergency_breaks()` is now **true** (the WIP's doing), so it takes
   `grapheme_boundaries(remaining).get(1)` — **the first grapheme only**.
4. The loop repeats, emitting **one character per line**.

Chrome fills the line and wraps at the last grapheme that fits. So the WIP makes
`overflow-wrap: anywhere` wrap at 1 char/line rather than not at all — closer to
correct in kind, still wrong in degree.

## The test that cannot fail for the right reason

The WIP ships an engine-driving test asserting the probe box gets **taller**
under `break-all` / `anywhere` / `break-word` than under `normal`. One character
per line is *maximally* taller, so **the test passes on the broken behavior**.
This is the same mistake class logged on this seat in 2026-08 for the subpixel
work ("wrote a falsifier that could not fail"). The assertion must state the
line *count* Chrome would produce, not merely the direction of the change.

## An instrument artifact I generated, and voided

Master's board was reproduced **bit-exact** before anything was touched:
**7/25 (28.0%)**, every per-case diff identical to n21's committed receipt. The
instrument is deterministic.

I then built `-p rustkit-engine`, re-ran, and got a board **byte-identical to
master — zero pixels changed on all 30 cases.** That reading is **VOID, not a
finding**: `scripts/wpt_tier1.py` does **not** build. It shells out to a
prebuilt `target/release/parity-capture` and only errors if the binary is
absent — it never checks that the binary is *current*. Both runs therefore used
the same stale binary, so the second run could not have measured the change.

The near-miss is worth the ledger line: the null result had a ready-made story
("14 of 18 are @font-face-blocked, so of course nothing moved") that would have
made a broken measurement look like a confirmed one. Same shape as n20's Ahem
staging, where zero-pixels-changed *was* the receipt. **A runner that consumes a
build artifact it does not produce should assert the artifact is newer than the
sources it was built from**; that guard does not exist and is a cheap follow-up.

The honest post-WIP board number was **not taken this session** — the cap was
reached during the rebuild. It is not estimated below.

## Scope consequence

The plumbing is necessary and should land, but it is **not sufficient** for the
slice-0 lane. The remaining work is contained and specced:

- `word-break: break-all` → grapheme opportunities belong in
  `break_opportunities` (they are *real* opportunities used in normal
  line-filling).
- `overflow-wrap: anywhere | break-word` → **last-resort only**; the fix belongs
  in `wrap_segment`'s emergency arm, which must take as many graphemes as **fit**
  the remaining width, not one. (`anywhere` additionally affects min-content
  intrinsic sizing; `break-word` does not — not addressed here.)

Standing caveat from n20/n21: **14 of the 18 Tier-1 fails are
`blocked_by: @font-face`**, so the WPT board is not the meter for this lane until
web fonts load. Correctness here is justified by real-site parity, not by K/N.
