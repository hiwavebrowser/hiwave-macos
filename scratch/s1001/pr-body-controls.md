## What

Atlas queue item 1 (semantic controls), from the 2026-09-30 outside review §7.

The control-paint path (`DisplayList::render_form_control`) matched `FormControlType::TextInput { value, placeholder, .. }` and dropped `input_type`, then pushed the same `DisplayCommand::TextInput` for every text-like control. Two results:

- **`<input type=password>` painted its value as plain text**, and the value was written into every display-list dump (`--dump-display-list` prints unmodelled commands with `Debug`).
- **`<select>` painted as a text input showing one option**: no arrow on a drop-down, and a list box (`size > 1` or `multiple`) was one label centred in an empty frame.

## Change

- **rustkit-layout:** `DisplayCommand::TextInput` carries `kind: TextControlKind` (`Text`, `Password`, `TextArea`, `MenuList`). A password is masked **when the display list is built**, one U+2022 per character, so the value is in no display command, dump or frame. New `DisplayCommand::ListBox` carries every option, the selected rows, and the row height layout sized the box with (`list_box_row_height`, one function for both).
- **rustkit-engine:** `FormControlType::Select` carries `selected`: every option with `selected` under `multiple`, only the last one otherwise, none when no option has it (HTML §4.10.7; the first-option fallback is the drop-down's rule and stays in `selected_index`).
- **rustkit-renderer:** `MenuList` draws the arrow and seats the label 4px inside the padding edge. `ListBox` draws the frame, the rows clipped to the inside of the frame, and the selected-row highlight.

Geometry and colours are measured on pinned Chrome for Testing 148 (the form-controls baseline and a 9-control probe page): arrow about 7.5×4, centred 8.75px inside the right edge at mid-height, the same on a bare 137×19 select and on an 18px `padding: 8px 12px` one; option rows start inside the border; selected row `rgb(206,206,206)` with `rgb(16,16,16)` text (list box without focus).

## Tests (7)

- **3 engine tests through `build_layout_from_document` + `DisplayList::build`. All 3 FAIL on develop 9c701ab** (run there with the test module appended, then restored) and pass here:
  - `a_password_value_never_reaches_the_display_list`: neither `hunter2secret` nor `swordfish` (`type="PASSWORD"`) appears in any command; 13 and 9 bullets do; a text input's value and a password's placeholder still paint.
  - `a_list_box_reaches_paint_with_every_option_and_its_selection`: `multiple` keeps both selected rows, single keeps the last, no `selected` means none, `multiple` without `size` is a list box.
  - `a_drop_down_reaches_paint_as_a_menu_list`: select is `MenuList`, input `Text`, textarea `TextArea`.
- 2 layout tests: one bullet per **character** (not byte) with the caret index kept; the `ListBox` command's rect, rows and 16px row height, and `size` 0/1 staying a drop-down.
- 2 renderer tests: arrow centre against Chrome's pixels on both select sizes; row rects stacking from the inside of the frame, with the fourth of four options past a three-row frame.

Suites on head 90be88d: **rustkit-layout 580/580, rustkit-renderer 90/90.** rustkit-engine: the 23 control, form and input tests pass serially (`--test-threads=1`), including the pre-existing `a_select_shows_its_selected_option`. The full engine suite run in parallel at machine load 13 to 18 gave 231 passed and 38 failed, **every one of the 38 the `GPU test guard: waited …` timeout** (`lib.rs:105`, tests queueing for the GPU), none an assertion and none in a control test. A full serial run did not fit the session, so CI is the full-suite evidence for this PR.

## Campaign receipt

Arms: develop **9c701ab** (this branch's base) vs fix. The fix binary was built from this tree before a rustfmt-only pass over the new lines; head **90be88d** differs from it by whitespace only. develop moved to b946849 (#390) during the session; this branch does not include it.

```
all:      develop 2026-10-01T00:32:08  26/26  avg 1.1622 | fix 2026-10-01T00:22:51  26/26  avg 1.1612 | 23/26 identical
builtins: develop 2026-10-01T00:33:14   5/5   avg 1.9052 | fix 2026-10-01T00:23:37   5/5   avg 1.9072 |  4/5 identical
```

The three cases that moved are the three with these controls:

| case | develop | fix | why |
|---|---|---|---|
| form-elements | 0.9875 | **0.9535** | password `short` paints five bullets, as Chrome does |
| form-controls | 3.2442 | **3.2405** | drop-down arrows; list box paints its three visible rows |
| settings | 2.0819 | 2.0919 (**worse by 0.01**) | see below |

**settings is worse by 0.01 and I am not hiding it.** Its three first-viewport selects now have Chrome's arrow and label inset, but each select box is about 22px narrower than Chrome's (Chrome 92 wide, RustKit about 70): the intrinsic width of a `<select>` ignores author horizontal padding. That is on develop too and is not changed here. With the box starting 22px too far right, moving the label 4px toward Chrome's inset and adding the arrow changes more pixels than it matches (per-select differing pixels vs Chrome: 1311 → 1339, 1673 → 1691, 811 → 830). Follow-up below.

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): **holds on both arms** ("none worse than the committed floor"), settings line `geo_fails=243` on both. Geometry is identical for all 26 cases. Paint moved on the same three: form-elements 0.96386 → 0.96421, form-controls 0.95577 → 0.95571, settings 0.95071 → 0.95061.

<details><summary>Per-case diff_pct (all 26), develop 9c701ab vs fix</summary>

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
| form-controls | 3.2442 | 3.2405 |
| form-elements | 0.9875 | 0.9535 |
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
| settings | 2.0819 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| specificity | 0.6015 | 0.6015 |
| sticky-scroll | 0.6575 | 0.6575 |
</details>

WPT tier-1 not run: `third_party/wpt` is not synced in this worktree.

## Board

Not run. The session's one release build took 39 minutes under load (the same build is 6 minutes quiet), and that left no room for a board A/B. The change only reaches pages with a password value, a list box or a drop-down in the first viewport; I did not census the board sites for those. The 22:48 quiet board on develop af4b95d stands at 28/60.

## Limits, stated

- **Layout widths are not changed here, and two are wrong against Chrome** (both on develop already): a list box is `widest option + 2` where Chrome is `widest + 4 + 2` (option padding), so the widest label's last glyph is clipped by about 2px now that rows are painted; a drop-down's width ignores author horizontal padding (the settings case above). Next PR, with the Chrome numbers already measured (39.05 / 43.05 list boxes; 155 for the padded drop-down).
- Chrome's default rows are 17px on a page without `box-sizing: border-box` (and a bare input is 153×21, not 149×19). Layout's 16 / 149×19 were calibrated on form-controls, which sets `* { box-sizing: border-box }`. Not changed here.
- Not in this PR: `<optgroup>`, disabled options and controls, focus/hover colours, `appearance: none` (a select with `appearance: none` still gets the arrow), scrolling a list box, keyboard or pointer selection. The frame colour and square corners are the existing text-input frame (Chrome: 1px `#767676`, 2px radius).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
