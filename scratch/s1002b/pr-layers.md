Follows #433 (the shorthand's colour reset), which merged while this was being measured. The base arm of the receipts is #433's head `8f78611`; develop `33d7368` is its merge commit and has the same tree (`git diff 8f78611 33d7368` is empty), so that arm is develop.

## What

A layer of the `background` shorthand kept only its image, and only when the layer began with it. Everything else the layer says was dropped:

- `background: url(a.png) no-repeat center / cover` tiled from the top left at the image's own size.
- `background: no-repeat center url(a.png)` had no image at all.
- `background: linear-gradient(...) no-repeat 0 100% / 100% 2px` (an underline, a progress bar, a divider) painted nothing: the gradient parser was handed the whole layer and refused it.
- A `url(...)` ended at the first `)`, which a `data:` URL may contain.

`background-position` also read its keywords by place instead of by axis: `bottom` alone meant "right edge", and `top right` put `top` on the horizontal axis.

A fixture on the hub branch (`scratch/s1002b/bgsh3.html`, twenty 120 x 60 boxes; the image is a 20 x 20 `data:` PNG with four coloured quadrants, and gradients). Share of each box's pixels that differ from pinned Chrome 148 (`scratch/s1002b/box_cmp.py`):

```
__FIXTURE__
```

The one left is a url image at a length position (`10px 20px`): the display list's image command carries the position as two fractions, so a length is dropped and the image sits at the corner. Gradients take the length (rows 8 and 17). It is the same on develop for the longhand, and it goes with the fetch PR, where sprites need it.

## Change

`crates/rustkit-engine/src/lib.rs`:

- `parse_background_shorthand_layer` reads one layer token by token (css-backgrounds-3 §3.10): the image wherever it stands (`url()` to its closing parenthesis, or a gradient), the position with an optional `/ size`, one or two repeat keywords (`repeat no-repeat` is `repeat-x`), the attachment (read, not used), one or two boxes (the first sets origin and clip, the second the clip), and the colour. The `"background"` arm of `apply_style_property` uses it; `background-image` still takes the image alone.
- `parse_background_position`: one value centres the other axis, and `top` / `bottom` are vertical; two keywords are read by axis in either order; the three- and four-value form (`left 10px top 20px`) keeps an offset from `left` or `top`.

Not in this PR:

- An offset from the far edge (`right 5px bottom 5px`): the position value cannot say "from the far edge" yet, so the image sits on that edge.
- A token the layer parser does not know (`image-set()`, a `calc()` length, `em` units) is skipped; Chrome would use it or drop the declaration.
- **A CSS background image is never fetched.** Only `<img>` elements are discovered (`Engine::discover_images`); a `background-image: url(...)` reaches the display list and is painted only if it is a `data:` URL. This PR is the part that has to land first: once images are fetched, a `no-repeat` sprite that tiles would be worse than no image. The fetch is the next PR.
- A longhand that arrives before its layer exists (`background-size: cover` in one rule, `background-image` in a later one) is still lost, as on develop.

## Tests

Three engine tests:

- `the_background_shorthand_sets_each_layers_position_size_and_repeat`: image first and image last, with and without spaces around the slash, a colour beside a gradient with lengths, two layers each keeping its own, the initial values for what a later shorthand does not name, and a longhand after the shorthand.
- `a_background_position_reads_its_keywords_by_axis`.
- `a_shorthand_layers_url_is_taken_whole`: a `data:` URL holding `rgb(0,0,0)`, and a layer with no image.

Fail-first: the first two spliced into develop's test module, non-test code untouched (`scratch/s1002b/failfirst_layers.py`). The third calls the new function, so it cannot compile there.

```
__FAILFIRST__
```

__SUITES__

## Receipt

Both arms were release-built from clean sources against the same target directory and run on the develop worktree's fixtures (`scratch/s1001c/camp3.py`). develop is `__BASE__` (the same tree as develop `33d7368`); fix is `__HEAD__`.

```
__SUMMARY__
```

__RATCHET__

<details><summary>Per-case diff_pct, scope all (develop | fix)</summary>

| case | develop | fix |
|---|---|---|
__TABLE_ALL__

</details>

<details><summary>Per-case diff_pct, scopes builtins and micro (develop | fix)</summary>

| case | develop | fix |
|---|---|---|
__TABLE_REST__

</details>

## Real sites

RustKit frames only, each site captured develop, fix, develop, fix at 1280x800 (`scratch/s1001g/ab.py`). "within" is the difference between two captures of the same binary (the site's own variance); "across" is one arm against the other, all four pairs.

```
__FRAMES__
```

__MOVERS__

🤖 Generated with [Claude Code](https://claude.com/claude-code)
