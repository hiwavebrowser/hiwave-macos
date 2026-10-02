**Draft: do not merge yet.** The button's geometry is right and the campaign is clean, but giving a button its default face through the cascade exposes a bug that predates this PR: the engine's `background` shorthand never clears `background-color`. On a button with element children, `background: none`, `background: 0 0`, `background: unset`, `background: initial` and `all: unset` all leave the grey face (fixture `scratch/s1002a/bgreset.html` on the hub branch, 7 of 12 resets wrong against Chrome 148). weather.com's "More" navigation button shows it. The shorthand fix goes in first as its own PR; then this one gets a fresh receipt and leaves draft.

## What

Every unstyled push button was 8px too wide and 2px too short, ignored `line-height`, and could not be reset.

Chrome's UA sheet gives a button `padding: 1px 6px`, a 2px border, `box-sizing: border-box` and `line-height: normal`. The engine's default style gave it none of these, and layout stood in a fixed figure for a style with no padding or border: label + 24 wide, 19 tall. That figure was measured on a fixture whose reset sets `padding: 0`, so it is right there and wrong for a bare button, and a `padding: 0; border: 0` reset (Tailwind's base styles, most resets) still got label + 24 where Chrome gives the label alone.

Two pages on the hub branch, pinned Chrome 148 against both arms (`scratch/s1002a/btn_cmp.py`):

```
button (Arial 13.333px "Go")        Chrome 148       develop f60d1d6    fix
bare                                33.80 x 21       41.79 x 19          33.79 x 21
padding: 0; border: 0               17.80 x 15       41.79 x 19          17.79 x 15
border: none                        29.80 x 17       41.79 x 19          29.79 x 17
padding: 8px 16px                   53.80 x 35       49.79 x 31          53.79 x 35
line-height: 30px                   33.80 x 36       41.79 x 19          33.79 x 36
font: 13px/16px Helvetica           33.34 x 22       41.34 x 18.52       33.34 x 22
no label                            16 x 6           24 x 19             16 x 6
<input type=submit>                 57.50 x 21       65.50 x 19          57.50 x 21
<input type=button>, font 13px/16px 33.34 x 21       41.34 x 18.52       33.34 x 21
<button><span>Go</span></button>    33.80 x 21       17.79 x 20          33.79 x 21

boxes more than 0.5px off (x, y, w or h)
scratch/s1001h/l0/btn.html          -                15 of 15            2 of 15
scratch/s1002a/btn2.html            -                12 of 12            7 of 12
scratch/s1001h/l0/l0-grid-flex.html -                25 of 25            2 of 25
```

What is left off, each one cause:

- `btn.html`, 2 boxes, 0.62 and 0.66px in y: a Helvetica button on a line whose baseline comes from a `line-height: 30px` button. Chrome splits that button's 15px of leading 7 above and 8 below; RustKit splits it 7.5 and 7.5.
- `btn2.html`, 7 boxes: one button has `box-sizing: content-box; width: 100px; height: 40px` (Chrome 116 x 46). A control's explicit width and height are taken as its border box whatever `box-sizing` says, on develop too. The other six share its line or sit under it, and move by its 16px and 6px.
- `l0-grid-flex.html` (the fixture of the L0 design's section 6), 2 boxes: the percentage-height item in an auto row, which is outside this change. The other 23 were off only because the button took 8px from the wrapped text beside it.

## Change

`crates/rustkit-engine/src/lib.rs`:

- `is_push_button`: `<button>`, or `<input type=button|submit|reset>`.
- The UA arm gives a push button `padding: 1px 6px`, 2px borders, `box-sizing: border-box`, ButtonFace (239, 239, 239) and the frame's grey (118, 118, 118). An author's `padding`, `border`, `background` override them through the cascade as for any element; `border: none` zeroes the width as it already did.
- A push button does not take the page's `line-height` (the engine inherits it into any element whose own is `normal`); `line-height: inherit` on the button still does. An `<input>` button's line is always `normal`: Chrome computes `normal` for it under `font: 13px/16px`, where a `<button>` computes 16px.

`crates/rustkit-layout/src/lib.rs`:

- `button_border_box_width` and the `Button` arm of `form_control_intrinsic_size` always compose: label plus horizontal padding and border; line plus vertical padding and border. The line is `resolve_line_height`. A button with no label has no line.
- `form_control_baseline_hang` counts a button's bottom padding and border. A control's padding and border are folded into its content box, so `dimensions.padding` is zero and the hang was the descent alone: buttons of different padding on one line shared a bottom edge instead of a baseline. A button with no label hangs only its padding and border.
- `render_form_control` paints no frame for a button whose border width is zero. The 1px stand-in frame was drawn over the label of a `padding: 0; border: 0` button.

Text fields, textareas and selects are untouched: they keep their calibrated sizes and the old hang. They are the next PR (the same rule, with Chrome's `padding: 1px 2px` and inset border).

Not in this PR:

- `box-sizing: content-box` on a control (above).
- A button whose author background is `none` or `transparent` still paints ButtonFace in the control leaf (the leaf substitutes it when alpha is 0). Chrome drops the themed look and paints the 2px outset border. Unchanged from develop.
- A button with element children is a normal box, so its frame is a 2px grey border where Chrome's themed frame is about 1px with rounded corners.

## Tests

Five new, one replaced.

`rustkit-layout`:

- `a_button_is_its_label_and_line_inside_its_padding_and_border`: the bare, reset, `line-height: 30px`, `padding: 8px 16px` and no-label sizes above.
- `buttons_on_one_line_share_the_baseline_of_their_labels`: bare, padded, reset and empty buttons on one line sit 7, 0, 10 and 19 under the tallest's top, through `layout()` and through the collapse path the engine's page layout runs.
- `a_button_without_a_border_paints_no_frame`.
- `a_reset_buttons_min_content_is_its_widest_word` replaces `a_bare_buttons_min_content_keeps_the_ua_well`, which pinned the 24px stand-in for a style with no padding or border.

`rustkit-engine`:

- `a_push_button_has_chromes_default_box`: the default box, face and `line-height: normal` for `<button>` and the three input types under a parent with `line-height: 20px`; a reset takes the box away; `line-height: inherit` brings the page's back; an author's line-height reaches a `<button>` and not an `<input>` button; a text field gets none of it.

Fail-first: the new tests spliced into develop's test modules, non-test code untouched (`scratch/s1002a/failfirst_btn.py`):

```
== layout tests on develop
test flex::tests::test_explicitly_sized_button_in_a_flex_row_keeps_its_size ... ok
test tests::a_bare_buttons_min_content_keeps_the_ua_well ... ok
test tests::a_flex_items_automatic_minimum_floors_a_button_at_its_widest_word ... ok
test tests::a_buttons_min_content_is_its_widest_word_not_its_whole_label ... ok
test tests::a_fixed_height_button_centres_its_line_about_the_baseline ... ok
test tests::a_buttons_min_content_is_strictly_narrower_than_its_max_content ... ok
test tests::an_explicit_pixel_width_wins_over_a_buttons_min_content ... ok
test tests::a_nowrap_button_carries_its_whole_label_into_min_content ... ok
test tests::a_flex_container_of_buttons_measures_the_buttons_and_the_gap ... ok
test tests::a_button_is_its_label_and_line_inside_its_padding_and_border ... FAILED
test tests::a_button_without_a_border_paints_no_frame ... FAILED
test tests::a_reset_buttons_min_content_is_its_widest_word ... FAILED
test tests::a_single_word_button_has_the_same_min_and_max_content ... ok
test tests::test_styled_button_composes_its_font_line_with_author_padding ... ok
test tests::buttons_on_one_line_share_the_baseline_of_their_labels ... FAILED
thread 'tests::a_button_is_its_label_and_line_inside_its_padding_and_border' (16592033) panicked at crates/rustkit-layout/src/lib.rs:13215:9:
reset width is the label (17.786015), got 41.786015
thread 'tests::a_button_without_a_border_paints_no_frame' (16592034) panicked at crates/rustkit-layout/src/lib.rs:13319:9:
thread 'tests::a_reset_buttons_min_content_is_its_widest_word' (16592041) panicked at crates/rustkit-layout/src/lib.rs:13147:9:
a reset button's min-content is its widest word (46.836327), got 70.83633
thread 'tests::buttons_on_one_line_share_the_baseline_of_their_labels' (16592216) panicked at crates/rustkit-layout/src/lib.rs:13274:13:
collapse_path=false: the bare button sits 7 under the padded one's top, got 14
test result: FAILED. 12 passed; 4 failed; 0 ignored; 0 measured; 587 filtered out; finished in 11.83s
error: test failed, to rerun pass `-p rustkit-layout --lib`
restored: True
== engine test on develop
test button_children_tests::a_push_button_has_chromes_default_box ... FAILED
thread 'button_children_tests::a_push_button_has_chromes_default_box' (16595509) panicked at crates/rustkit-engine/src/lib.rs:16881:13:
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 286 filtered out; finished in 14.97s
error: test failed, to rerun pass `-p rustkit-engine --lib`
restored: True
```

On the branch: `cargo test -p rustkit-layout --lib` 602/602; `cargo test -p rustkit-engine --lib button_children_tests` 4/4. The whole engine suite was not run locally (other lanes were building; its GPU guard times out under load), so CI's `unit-suites` is its first full run.

## Campaign receipt

Arms: develop **f60d1d6** (this branch's base) vs fix **19ec405**, both release binaries built in this session with every workspace source touched first.

```
all:      develop 2026-10-02T01:25:31 26/26 avg 1.1071 | fix 2026-10-02T01:34:27 26/26 avg 1.0930 | 25/26 identical
builtins: develop 2026-10-02T01:21:40  5/5  avg 1.8139 | fix 2026-10-02T01:31:10  5/5  avg 1.8139 | 5/5 identical
micro:    develop 2026-10-02T01:20:59 13/13 avg 0.6548 | fix 2026-10-02T01:30:32 13/13 avg 0.6266 | 12/13 identical
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): the lines that differ between the arms, develop then fix:

```
dev: form-controls: geo_fails=43 paint=0.95576 discrete=0
fix: form-controls: geo_fails=32 paint=0.95949 discrete=0
```

The one mover is `form-controls`, 3.2388 -> 2.8713%, and its geometry gate goes from 43 failures to 32 with none new (`scratch/s1001h/gate_diff.py`). The eleven are its three buttons and two input buttons, which were label + 24 wide (55.87 for Chrome's 35.88) because the page's reset leaves them a border and no padding, and the boxes after them on the same lines. `settings` (243 geometry failures) does not move: its buttons carry author padding and border and composed correctly already.

<details><summary>Per-case diff_pct (all 26), develop f60d1d6 vs fix 19ec405</summary>

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
| form-controls | 3.2388 | 2.8712 | moved
| form-elements | 0.9535 | 0.9535 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-backgrounds | 1.0133 | 1.0133 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| image-gallery | 0.5217 | 0.5217 |
| images-intrinsic | 0.3267 | 0.3267 |
| new_tab | 1.1019 | 1.1019 |
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
| new_tab | 1.1019 | 1.1019 |
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| combinators | 0.6547 | 0.6547 |
| form-controls | 3.2388 | 2.8712 | moved
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

RustKit frames, develop / fix / develop / fix on each live URL at 1280x800 (`scratch/s1001g/ab.py` on the hub branch), with the campaign arms running on the same machine (load 12 to 14). "Across arms" is the share of pixels where any channel differs by more than 8. The movers were then scored with the board's own diff against the Chrome frames stored by the 2026-10-01 14:54 quiet board.

```
site         within develop  within fix  across arms              against Chrome: develop -> fix
weather          0.00%         0.00%      0.83% (all four pairs)   30.53% -> 30.53%
walmart          0.00%         0.00%      0.26% (all four pairs)   90.74% -> 90.73%
squarespace        -           0.00%      1.94% (two pairs)        77.29% -> 77.29%
google, youtube, wikipedia, lyft, reddit, x, microsoft, apple: 0.00% across arms (pixel-identical)
shopify            -           0.00%      0.00% (one develop capture; the other timed out)
facebook         0.00%           -        0.00% (three captures identical; the fourth timed out)
linkedin         1.55%         1.55%      0.00% to 1.55% (its own variance)
bing             0.24%         0.00%      0.00% to 0.24% (its own variance)
yahoo            0.00%        19.15%      0.00% to 19.15% (its own variance: one fix capture is a different page)
netflix          4.82%         2.03%      2.68% to 5.00% (its own variance: the hero rotates)
github, cnn, instagram: not captured on either arm within the binary's 30 s, in two attempts
```

- **weather is a regression in paint, and it is why this PR is a draft.** Its "More" navigation button (a button with element children) now has the grey button face; Chrome's has none. The site removes the face with a `background` shorthand, and the engine's `background` arm only ever sets a colour, it never clears one. The board's diff does not move (30.53% on both arms, the page differs there anyway) and no check changes.
- walmart: a band under its header moves by a few pixels. Both arms are 90% off Chrome.
- squarespace: 1.94% across arms on the two pairs captured. I did not look at where.
- No board check changes on any site.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
