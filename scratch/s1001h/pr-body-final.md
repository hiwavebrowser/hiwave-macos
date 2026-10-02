## What

A grid item that is a flex container did not fill its row, and did not align its items in the row's height.

Phase 9 of grid layout runs the item's flex layout to learn its content height. An auto-height flex container writes that content height over the grid area it was handed, and nothing gave a stretched item its row back unless the row-repair pass (Phase 9.5) happened to change some row. When that pass did stretch the item, its items stayed where the content-height pass had aligned them.

Reduced page (`scratch/s1001h/l0/stretch.html` on the hub branch): a grid row whose height comes from a `height: 100px` sibling, beside flex containers with `align-items: center` holding one 30x20 box. Pinned Chrome 148 against both arms:

```
box                           Chrome 148            develop f657cf2       fix
flex row item                 200 x 100 at y=10     200 x 38              200 x 100
  its 30x20 child             y=50                  y=19                  y=50
flex column item (justify)    200 x 100 at y=10     200 x 38              200 x 100
  its 30x20 child             y=50                  y=19                  y=50
flex item, one line of text   200 x 100 at y=120    200 x 38              200 x 100
  its text                    y=160                 y=129                 y=160
flex item, align-self: start  200 x 38              200 x 38              200 x 38
boxes more than 0.5px off     -                     6 of 11               0 of 11
```

`new_tab` has the second half: its `.shortcut` rows are flex rows in a grid, and the ones sharing a row with a taller shortcut had every child 5px above Chrome's.

## Change

`crates/rustkit-layout/src/grid.rs`, `flex.rs`:

- Phase 9, flex arm: the content height still goes to `real_heights` (what Phase 9.5 sizes rows from). If `align-self` resolves to `stretch` and the height is `auto` (`stretches_to_its_row`), the item keeps the area height Phase 8 gave it.
- Phase 9.8 (new, last): a flex-container item whose used height ended above its content height lays its items out again at that height, through `flex::layout_flex_container_at_used_height`. That is the entry point a stretched flex item that is itself a flex container already takes (#300); the grid never called it.
- An item with `align-self: start` (or any non-stretch alignment), `height: fit-content`, or an explicit height is untouched. An `aspect-ratio` item still gets its height from Phase 9.6 and is then laid out at it.

Cost: one more flex layout for each flex-container grid item that is shorter than its row. Items as tall as their row are not laid out again.

Not in this PR: a grid item that is itself a grid has the same loss of its stretched height (`layout_grid_container` re-derives an auto height from its rows). The page above has no such item; it wants its own test.

## Tests

Four in `grid.rs`:

- `a_flex_grid_item_fills_its_row_and_aligns_in_it`: beside a 100px sibling the flex item is 100 tall and its centred 20px child's top is at 40.
- `a_column_flex_grid_item_justifies_in_its_row`: the same for a column container with `justify-content: center`.
- `a_flex_grid_item_realigns_after_its_row_grows`: two flex items share an auto row that Phase 9.5 grows to 60; the short one fills it and centres its child at 20.
- `a_start_aligned_flex_grid_item_keeps_its_content_height`: the opt-out. This one is a guard and passes on develop.

Fail-first, with the two production changes switched off under the new tests (`scratch/s1001h/failfirst_gf.py`):

```
test grid::tests::a_start_aligned_flex_grid_item_keeps_its_content_height ... ok
test grid::tests::a_column_flex_grid_item_justifies_in_its_row ... FAILED
test grid::tests::a_flex_grid_item_realigns_after_its_row_grows ... FAILED
test grid::tests::a_flex_grid_item_fills_its_row_and_aligns_in_it ... FAILED
thread 'grid::tests::a_column_flex_grid_item_justifies_in_its_row' panicked at crates/rustkit-layout/src/grid.rs:7201:9:
the flex item fills the 100px row, got 20
thread 'grid::tests::a_flex_grid_item_realigns_after_its_row_grows' panicked at crates/rustkit-layout/src/grid.rs:7263:9:
the short item fills the 60px row, got 20
thread 'grid::tests::a_flex_grid_item_fills_its_row_and_aligns_in_it' panicked at crates/rustkit-layout/src/grid.rs:7173:9:
the flex item fills the 100px row, got 20
test result: FAILED. 1 passed; 3 failed; 0 ignored; 0 measured; 594 filtered out; finished in 0.31s
restored: True
```

Suites at 026fcc5: rustkit-layout 598/598 (plus its integration tests, 5/5). Only rustkit-layout changed; the other crates' suites were not run locally, CI runs them.

## Campaign receipt

Arms: develop **f657cf2** (this branch's base) vs fix **026fcc5**, both release binaries built in this session with every workspace source touched first.

```
all:      develop 2026-10-01T22:05:31 26/26 avg 1.1224 | fix 2026-10-01T22:44:55 26/26 avg 1.1159 | 25/26 identical
builtins: develop 2026-10-01T22:01:31  5/5  avg 1.8936 | fix 2026-10-01T22:41:02  5/5  avg 1.8597 | 4/5 identical
micro:    develop 2026-10-01T22:00:46 13/13 avg 0.6548 | fix 2026-10-01T22:40:12 13/13 avg 0.6548 | 13/13 identical
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): the lines that differ between the arms, develop then fix:

```
dev: about: geo_fails=66 paint=0.93940 discrete=0
fix: about: geo_fails=60 paint=0.93940 discrete=0
dev: new_tab: geo_fails=17 paint=0.94520 discrete=0
fix: new_tab: geo_fails=6 paint=0.94969 discrete=0
```

<details><summary>Per-case diff_pct (all 26), develop f657cf2 vs fix 026fcc5</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6742 | 3.6742 |
| article-typography | 4.7857 | 4.7857 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| card-grid | 1.3074 | 1.3074 |
| chrome_rustkit | 1.1234 | 1.1234 |
| combinators | 0.6547 | 0.6547 |
| css-selectors | 1.3299 | 1.3299 |
| flex-positioning | 0.6327 | 0.6327 |
| form-controls | 3.2388 | 3.2388 |
| form-elements | 0.9535 | 0.9535 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-backgrounds | 1.0133 | 1.0133 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| image-gallery | 0.5217 | 0.5217 |
| images-intrinsic | 0.3267 | 0.3267 |
| new_tab | 1.5003 | 1.3309 | moved
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 0.4354 | 0.4354 |
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| specificity | 0.6015 | 0.6015 |
| sticky-scroll | 0.6575 | 0.6575 |

</details>

<details><summary>Per-case diff_pct (builtins, micro)</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6742 | 3.6742 |
| chrome_rustkit | 1.1234 | 1.1234 |
| new_tab | 1.5003 | 1.3309 | moved
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| combinators | 0.6547 | 0.6547 |
| form-controls | 3.2388 | 3.2388 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| images-intrinsic | 0.3267 | 0.3267 |
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 0.4354 | 0.4354 |
| specificity | 0.6015 | 0.6015 |

</details>

## Real sites

RustKit frames, develop / fix / develop / fix on each live URL at 1280x800 (`scratch/s1001g/ab.py` on the hub branch). "Across arms" is the share of pixels where any channel differs by more than 8. apple was then scored with the board's own diff against the Chrome frames stored by today's 14:54 quiet board.

```
site        within develop  within fix  across arms             against Chrome: develop -> fix
apple            0.00%        0.00%    36.09% (all four pairs)   54.81% -> 41.65%
wikipedia, walmart, x, shopify, microsoft, yahoo, bing, weather, youtube, reddit, github, squarespace, lyft: 0.00% across arms (pixel-identical)
facebook         0.00%          -       0.00% (three captures identical; the fourth timed out)
cnn                -            -       0.00% (one develop/fix pair; the other two captures timed out)
google           0.00%        0.00%     4.09%, then 0.00% on a second run of four, then 0.00% to 4.09% on a third
linkedin        40.81%        2.64%     0.00% to 40.97% (its own variance: it serves two layouts)
netflix          2.30%       12.63%     5.00% to 12.79% (its own variance: the hero rotates between loads)
instagram: not captured on either arm within the binary's 30 s
```

- **apple: 13 points closer to Chrome, still a fail.** Its hero is a grid whose item is a flex container. On develop the hero panel stopped at its text (290px, the product image cut off at the top edge) and three stray navigation labels showed on white below it. With the fix the panel is 580px with the image in it and the labels are gone. Chrome's panel is 692px with the image centred; the remaining difference is the collapsed navigation bar and the image's position.
- **google is the request, not the binary.** The first four captures differed by arm (4.09%). The two frames are two different pages Google serves (two buttons under the search box, or four chips: "Create images", "Ask about files", "Brainstorm", "I'm feeling lucky"). Eight more captures gave both pages on the fix binary and identical frames across arms.
- No board check changes on any site.
- Capture time, seconds per load, develop then fix: no site is consistently slower on the fix arm (apple 22, 25 against 26, 25; yahoo 24, 25 against 21, 21; github 24, 19 against 22, 22). facebook finished in 23 to 27 s on develop and 26 s then a 30 s timeout on the fix arm, in that order, while the machine was at load 12 to 20: it is near the limit on both arms, and I could not separate the arms from the load.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
