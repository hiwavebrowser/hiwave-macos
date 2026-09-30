## What

An id followed by more of its compound never matched: `#a.x`, `#a:first-child`, `#a[data-k]`, `#a:not(.y)`, and the same inside `:is()`/`:not()`. All three subject-side parsers read everything after a leading `#` as the id:

- `simple_selector_matches_with_pseudo` compared the whole remainder (`a.x`) to the element's id;
- `SubjectCompound::parse` (the compiled matcher) built `IdOnly("a.x")`;
- `keys_for_compound` (rule-index keys) copied the quirk on purpose, so the index and the matcher would agree.

A tag in front (`p#a.x`) already worked, because it goes through the general part-by-part path. The ancestor path (`#a.x .c`) was already right.

The fix is a shared predicate, `is_bare_id`. A leading id takes the fast path only when it is the whole compound. Otherwise the compound goes through the general path, and the index key requires the leading id, which is still a necessary condition. An escaped remainder (`#a\:b`) keeps the whole-id reading, because the part scanners split on delimiters without knowing escapes. That behaviour is unchanged.

Real sheets do write these. apple's saved page has 647 `#globalnav.*` / `#globalnav:*` / `#globalnav[*]` selectors, bing's has 109 (`#id_mobile.a`), and wikipedia's has `#p-lang-btn.mw-…`.

**Also in this PR: the #379 compile follow-up.** develop `315fbb3` does not compile `rustkit-engine`: #379 moves `style` into `LayoutBox::new`, and #378's pseudo calls then borrow `&style`. This branch uses `&layout_box.style` in both places. #381 carries the identical two-line change, so whichever lands first, the other merges cleanly.

## Pins

`id_compound_selector_tests` (engine, headless):
1. `an_id_followed_by_a_class_matches`: `#a.x`, `p#a.x`, and the negative `#a.y`. **Fails first.**
2. `an_id_followed_by_an_attribute_or_pseudo_class_matches`: `#a[data-k]`, `#a:first-child`, plus negatives. **Fails first.**
3. `an_id_compound_matches_inside_is_and_not`: `:is(#a.x)`, `p:not(#a.x)`. **Fails first.**
4. `a_bare_and_an_escaped_id_still_match_whole`: `#b`, `#a\:b` (a guard; passes both ways).

rustkit-engine 308/308 (headless, serial).

## Campaign receipt

Head `9b96b1d` on develop `315fbb3`. The develop arm is `315fbb3` plus only the two-line compile fix above, since `315fbb3` itself does not build. Both arms are release `parity-capture` builds with every workspace crate recompiled.

- `parity_test.py --scope all`: develop run 2026-09-30T12:47:25, fix run **2026-09-30T12:49:47**. 26/26 and 26/26 pass, avg 1.176% vs 1.176%. **26/26 identical case for case.**
- `--scope builtins`: develop 2026-09-30T12:47:38, fix 2026-09-30T12:50:00: **5/5 identical** (avg 1.905%).
- `scratch/shelf302/ratchet_local.py`: output **identical** on both arms. Both exit 2 (tighten-eligible only), which is develop's existing state.

<details><summary>all: per-case diff_pct (dev-315fbb3 vs idc-9b96b1d)</summary>

| case | dev-315fbb3 | idc-9b96b1d |
|---|---|---|
| new_tab | 1.57% | 1.57% |
| about | 3.67% | 3.67% |
| settings | 2.08% | 2.08% |
| chrome_rustkit | 1.12% | 1.12% |
| shelf | 1.08% | 1.08% |
| article-typography | 4.79% | 4.79% |
| card-grid | 1.31% | 1.31% |
| css-selectors | 1.38% | 1.38% |
| flex-positioning | 0.63% | 0.63% |
| form-elements | 0.99% | 0.99% |
| gradient-backgrounds | 1.01% | 1.01% |
| image-gallery | 0.52% | 0.52% |
| sticky-scroll | 0.66% | 0.66% |
| backgrounds | 1.22% | 1.22% |
| bg-solid | 0.21% | 0.21% |
| bg-pure | 0.00% | 0.00% |
| combinators | 0.65% | 0.65% |
| form-controls | 3.24% | 3.24% |
| gradients | 0.14% | 0.14% |
| gradient-no-radius | 0.52% | 0.52% |
| gradient-radius-only | 0.69% | 0.69% |
| gpu-gradient-regression | 0.39% | 0.39% |
| images-intrinsic | 0.33% | 0.33% |
| pseudo-classes | 0.43% | 0.43% |
| rounded-corners | 1.32% | 1.32% |
| specificity | 0.60% | 0.60% |

</details>

<details><summary>builtins: per-case diff_pct (dev-315fbb3 vs idc-9b96b1d)</summary>

| case | dev-315fbb3 | idc-9b96b1d |
|---|---|---|
| new_tab | 1.57% | 1.57% |
| about | 3.67% | 3.67% |
| settings | 2.08% | 2.08% |
| chrome_rustkit | 1.12% | 1.12% |
| shelf | 1.08% | 1.08% |

</details>

WPT tier-1 not run: `third_party/wpt` is not synced in this worktree.

## Real-site board

One interleaved round (develop `315fbb3`+compile fix = A, then `9b96b1d` = B), load 6, runs `trench/realsite/runs/20260930T1640Z-abidc-{1-A,2-B}` on the hub branch:

| site | develop | 9b96b1d |
|---|---|---|
| apple | 2 (LOOKS 59.65%) | 2 (59.65%) |
| bing | 1 (READABLE 10.9%) | 1 (13.3%, Chrome showed 46 vs 45 words: drift) |
| wikipedia | 2 (READABLE 81.5%) | 2 (81.5%) |

**±0 points.** apple's layout dump differs between the arms, but its first-viewport pixels are identical. Most of its `#globalnav.*` rules key off state classes that page JS sets (`globalnav-with-flyout-open`, `with-bag-count-*`), and RustKit doesn't run that JS yet. This is a correctness fix for a common selector shape, not a point flip.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
