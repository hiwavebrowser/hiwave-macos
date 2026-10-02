**Draft: do not merge yet.** The button's geometry is right and the campaign is clean, but giving a button its default face through the cascade exposes a bug that predates this PR: the engine's `background` shorthand never clears `background-color`. On a button with element children, `background: none`, `background: 0 0`, `background: unset`, `background: initial` and `all: unset` all leave the grey face (fixture `scratch/s1002a/bgreset.html` on the hub branch, 7 of 12 resets wrong against Chrome 148). weather.com's "More" navigation button shows it. The shorthand fix goes in first as its own PR; then this one gets a fresh receipt and leaves draft.

## What

Every unstyled push button was 8px too wide and 2px too short, ignored `line-height`, and could not be reset.

Chrome's UA sheet gives a button `padding: 1px 6px`, a 2px border, `box-sizing: border-box` and `line-height: normal`. The engine's default style gave it none of these, and layout stood in a fixed figure for a style with no padding or border: label + 24 wide, 19 tall. That figure was measured on a fixture whose reset sets `padding: 0`, so it is right there and wrong for a bare button, and a `padding: 0; border: 0` reset (Tailwind's base styles, most resets) still got label + 24 where Chrome gives the label alone.

Two pages on the hub branch, pinned Chrome 148 against both arms (`scratch/s1002a/btn_cmp.py`):

```
button (Arial 13.333px "Go")        Chrome 148       develop __BASE__    fix
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
__FAILFIRST__
```

__SUITES__

## Campaign receipt

Arms: develop **__BASE__** (this branch's base) vs fix **__HEAD__**, both release binaries built in this session with every workspace source touched first.

```
__SUMMARY__
```

__RATCHET__

__MOVERS__

<details><summary>Per-case diff_pct (all 26), develop __BASE__ vs fix __HEAD__</summary>

| case | develop | fix |
|---|---|---|
__TABLE_ALL__

</details>

<details><summary>Per-case diff_pct (builtins, micro)</summary>

| case | develop | fix |
|---|---|---|
__TABLE_REST__

</details>

## Real sites

__FRAMES__

🤖 Generated with [Claude Code](https://claude.com/claude-code)
