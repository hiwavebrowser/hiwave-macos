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

__LINKEDIN__

## Pins

`cascade_wire_tests`, through `build_layout_from_document`:
- **Fail first (5):** `revert_layer_rolls_back_to_the_layer_below`, `an_unlayered_revert_layer_rolls_back_to_the_layered_result`, `an_important_revert_layer_rolls_back_among_important_declarations`, `revert_layer_shows_what_the_same_layer_hid` (linkedin's `display: none` shape), `a_pseudo_element_reverts_its_layer_too`.
- **Guards (2, pass both ways):** `revert_layer_only_counts_when_it_wins_its_own_layer`, `revert_layer_leaves_the_layers_above_alone`.

linkedin puts its `revert-layer` under `@media`. An ad-hoc test build has no viewport and keeps no conditional rule, so that combination is checked on the saved page with a release build (above) rather than by a unit pin.

__TESTS__

## Campaign receipt

__RECEIPT__

🤖 Generated with [Claude Code](https://claude.com/claude-code)
