## What

Two fixes that linkedin's layered-CSS bundle needs together. It stacks its hero and topic pills with a nested grid-stack utility:

```css
.auya49 { display: grid; & > * { grid-area: 1/-1; min-width: 0 } }
```

**1. CSS nesting (`rustkit-cssparser`, de4586a).** The parser documented "does not support CSS nesting". It was worse than dropping the rule: the nested `}` closed the PARENT rule, whose declarations then got a garbage property, and the parent's own `}` glued onto the next selector, so **the rule after every nested block was lost too**. linkedin has 16 nested rules, each followed by a `.auya5h:empty…` rule that died with it.
- A `{` at the top level of a declaration block now opens a nested rule: `& > *`, `&:hover` (whose `:` had put the reader in value position), `.child` / `> .c` (relative, meaning `& .child` / `& > .c`), and nested `@media`/`@supports`/`@layer`, whose bare declarations apply to the parent.
- Nested rules flatten into ordinary rules in source order. Declarations before a nested rule stay with the parent. Later ones become a second parent rule after it (the nested-declarations rule).
- **How `&` resolves:** it's expanded per parent (`.a, .b > p { & span }` becomes `.a span, .b > p span`), because the matcher's `:is()` takes compound arguments only. An engine pin failed with the spec's `:is(<parent>)` form. Past 64 expanded selectors, the parent list stays one `:is()`. The two known differences from `:is()` are in the doc comment: each expanded selector keeps its own specificity, and `.c &` under a complex parent is stricter. Neither occurs on the board's sites.
- A custom property whose value holds a `{}` block stays a value.

**2. Negative grid lines count from the explicit grid (`rustkit-layout`, 78c3ebd).** CSS Grid §8.3. With no explicit columns, `-1` is line 1. It was resolved against the track count after the implicit fallback column existed. Also, "start given, end auto" set end = start + 1 before resolving, so `-1` ended at line 0. `grid-area: 1 / -1` on a template-less grid put every child in a stray second column (x=96, width 96 of 288) instead of stacking them. With only fix 1, the flat and nested forms were both still broken, so fix 1 alone would not have helped. Negatives are now resolved once, where the placement is built, before any implicit track exists.

## Pins

- `rustkit-cssparser`: `nested_style_rules_resolve_against_the_parent_and_the_next_rule_survives`, `nested_selector_forms`, `declarations_after_a_nested_rule_stay_with_the_parent_in_source_order`, `nested_group_rules_apply_to_the_parent` (**all 4 fail first**), `a_custom_property_value_may_hold_a_block`. 22/22.
- `rustkit-engine` (cascade + layout through `build_layout_from_document`): `a_nested_rule_styles_the_parents_child`, `a_nested_rule_under_a_complex_parent_list_matches`, `the_rule_after_a_nested_rule_still_applies`, `nested_grid_area_stacks_the_children`. 256/256: a parallel run had 4 GPU-guard timeouts under load 12, and all 24 `cascade_wire_tests` pass serially.
- `rustkit-layout`: `negative_lines_count_from_the_explicit_grid_so_grid_area_1_neg1_stacks` (**fails first**: x=96, w=96). 575/575. `a_new_web_font_set_invalidates_the_cache` flaked once in the parallel run and passes alone; it's a shared-cache test, unrelated.
- Minimal page (hub `scratch/s0930/nest-min.html`, release build): develop paints neither the nested children nor the rule after the block. With this PR, the children are green and stacked, and `.after` sits right under them.

## linkedin's layered variant, offline (honest)

The saved page (hub `scratch/li0930/li-new.html`, sheet inlined, scripts dropped) renders differently, but its first-viewport diff against pinned Chrome 148 is **unchanged at 82.9%**. The grid-stack wrapper around the hero (`div.auyip7.auya49`) is still **0 px tall** while its one child is 5697 px. A template-less grid's auto height ignores its stacked item there. That's a separate grid-sizing bug and the next item. The old CSS variant (served ~9 fetches in 10) has no nesting and is unaffected.

## Campaign receipt

Head `78c3ebd` on develop `e1e174f`. **Disclosed: the develop arm is `eef161e`** (#384's head, develop 1a016c4 + `light-dark()`), built this afternoon. It lacks only #382 (`#id.class` compounds) and #385 (CI-only). #382's own receipt was identical to develop on all three gates. At load 11–14, a fresh develop release build costs ~15 min.

- `parity_test.py --scope all`: develop run 2026-09-30T13:52:49, fix run **2026-09-30T16:53:55**. 26/26 and 26/26 pass, avg 1.1756% vs 1.1756%. **26/26 identical case for case.**
- `--scope builtins`: develop 2026-09-30T13:53:33, fix 2026-09-30T16:54:32: **5/5 identical** (avg 1.9052% vs 1.9052%).
- `scratch/shelf302/ratchet_local.py`: output **identical** on both arms (`settings: geo_fails=243 paint=0.95071 discrete=0`). Both exit 2, which is develop's existing state.

<details><summary>Per-case diff_pct (all 26), develop eef161e vs fix 78c3ebd</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6742 | 3.6742 |
| article-typography | 4.7857 | 4.7857 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| card-grid | 1.3068 | 1.3068 |
| chrome_rustkit | 1.1234 | 1.1234 |
| combinators | 0.6547 | 0.6547 |
| css-selectors | 1.3819 | 1.3819 |
| flex-positioning | 0.6327 | 0.6327 |
| form-controls | 3.2442 | 3.2442 |
| form-elements | 0.9875 | 0.9875 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-backgrounds | 1.0133 | 1.0133 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.6892 | 0.6892 |
| gradients | 0.1412 | 0.1412 |
| image-gallery | 0.5217 | 0.5217 |
| images-intrinsic | 0.3267 | 0.3267 |
| new_tab | 1.5685 | 1.5685 |
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 1.3221 | 1.3221 |
| settings | 2.0819 | 2.0819 |
| shelf | 1.0781 | 1.0781 |
| specificity | 0.6015 | 0.6015 |
| sticky-scroll | 0.6575 | 0.6575 |

</details>

WPT tier-1 not run: `third_party/wpt` is not synced in this worktree.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
