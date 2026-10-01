## What

`revert-layer` (CSS Cascade 5 §7.3.3) was dropped as an unknown value, which left the same layer's earlier declaration in force. Now, when the declaration that wins a property **within a layer** is `revert-layer`, that layer contributes nothing for the property and the result of the layers below it stands. In unlayered rules it rolls back to the layered result. Normal and `!important` declarations are resolved separately, each in its own layer order (important layers run in reverse). Elements and `::before`/`::after` are both covered; those are the only two paths that apply declarations.

It is one helper, `reverted_layer_properties`, called once per importance pass, plus a skip in each apply loop. No change to matching, sorting or the rule index.

**Cost (for the cascade lane):** an element that no `revert-layer` declaration reaches pays one length compare per matched declaration and no allocation. The helper returns an empty `Vec` and the apply loop's check is a single `is_empty()`.

**Limit (disclosed):** pairs are matched by property name, so a `revert-layer` longhand does not roll back a shorthand declared in the same layer (`background: red` + `background-color: revert-layer`). `revert` (roll back to the UA origin) is still not implemented.

## Where it shows

linkedin's layered-CSS variant (served on roughly 1 fetch in 10) hides its hero and shows it again on desktop, both in the `overrides` layer:

```css
@layer overrides {
  .auya89 { display: none }
  @media (min-width: 768px) { .auya83 { display: revert-layer } }
}
```

The rollback target is the `atoms` layer's `display: grid`. Without it the hero ("Welcome to your professional community", the sign-in button, the illustration) is never in the tree. The bundle has 12 `revert-layer` declarations: 8 `display`, 4 `background-color`.

**linkedin's saved layered page, offline.** Hub `scratch/li0930/li-new.html` (sheet inlined, scripts dropped; rendered through a real view, so its `@media` rules apply), first-viewport diff against pinned Chrome 148: develop 570e25d **82.9%**, this PR **52.9%**. The hero heading, sign-in button, legal text and "Join now" are now in the first viewport where Chrome has them. Still wrong and not this PR: the footer row painted across the top and the unsized sections below are #391 (`height: fit-content`); the heading is set larger than Chrome's and runs under the illustration, the buttons are unstyled grey, and the illustration paints as a flat box (causes not yet looked at). Resolving the one declaration by hand on #391's binary gave 52.7%, so the two PRs are independent and compose.

## Pins

`cascade_wire_tests`, through `build_layout_from_document`:
- **Fail first (5):** `revert_layer_rolls_back_to_the_layer_below`, `an_unlayered_revert_layer_rolls_back_to_the_layered_result`, `an_important_revert_layer_rolls_back_among_important_declarations`, `revert_layer_shows_what_the_same_layer_hid` (linkedin's `display: none` shape), `a_pseudo_element_reverts_its_layer_too`.
- **Guards (2, pass both ways):** `revert_layer_only_counts_when_it_wins_its_own_layer`, `revert_layer_leaves_the_layers_above_alone`.

linkedin puts its `revert-layer` under `@media`. An ad-hoc test build has no viewport and keeps no conditional rule, so that combination is checked on the saved page with a release build (above) rather than by a unit pin.

`rustkit-engine` lib: **266/266**, run serially (`--test-threads=1`) at 9a64034.

## Campaign receipt

- `parity_test.py --scope all`: develop run 2026-09-30T20:55:54, fix run **2026-09-30T22:13:21**. 26/26 and 26/26 pass, avg 1.1622% vs 1.1622%. **26/26 identical case for case.**
- `--scope builtins`: develop 2026-09-30T20:56:40, fix 2026-09-30T22:14:05: **5/5 identical** (avg 1.9052% vs 1.9052%).
- `scratch/shelf302/ratchet_local.py`: output **identical** on both arms (`settings: geo_fails=243 paint=0.95071 discrete=0`). Both exit 2, which is develop's existing state.

<details><summary>Per-case diff_pct (all 26), develop 570e25d vs fix 9a64034</summary>

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
| gradient-radius-only | 0.3429 | 0.3429 |
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
