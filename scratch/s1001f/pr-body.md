## What

On macOS, Chrome sets unstyled text in **Times**, `sans-serif` in **Helvetica** and `monospace` in **Courier**. RustKit laid out unstyled text and `sans-serif` in the system font and `monospace` in Menlo. Since #411 paint draws the face layout measured, so on develop every page that names no font, or only a generic, is drawn in the wrong face (wikipedia is one: its body is `sans-serif`).

Chrome 148 against layout, 28 characters at 16px (`scratch/s1001e/generic.html` on the hub branch, Chrome's numbers in `fx/generic-chrome.json`):

```
font-family                      Chrome  develop      fix  develop face -> fix face
(no font-family)                 199.52   220.78   199.51  .SFNS-Regular -> Times-Roman
sans-serif                       217.02   220.78   217.02  .SFNS-Regular -> Helvetica
serif                            199.52   199.51   199.51  TimesNewRomanPSMT -> Times-Roman
monospace                        268.84   269.72   268.84  Menlo-Regular -> Courier
"No Such Family", sans-serif     217.02   220.78   217.02  .SFNS-Regular -> Helvetica
"No Such Family", serif          199.52   199.51   199.51  TimesNewRomanPSMT -> Times-Roman
"No Such Family", monospace      268.84   269.72   268.84  Menlo-Regular -> Courier
"No Such Family"                 199.52   220.78   199.51  .SFNS-Regular -> Times-Roman
Helvetica                        217.02   217.02   217.02  Helvetica
Times                            199.52   199.51   199.51  Times-Roman
"Times New Roman"                199.52   199.51   199.51  TimesNewRomanPSMT
Menlo                            269.72   269.72   269.72  Menlo-Regular
system-ui                        220.78   220.78   220.78  .SFNS-Regular
cursive                          200.45   222.02   200.45  ComicSansMS -> Apple-Chancery
fantasy                          208.02   196.24   208.02  Impact -> Papyrus
```

## Change

- `rustkit-css`: the initial `font-family` is `INITIAL_FONT_FAMILY`, Times on macOS (it was `sans-serif`). `font-family: initial` gives the same value. Other platforms keep `sans-serif`.
- `rustkit-layout` `FontFamilyChain`, macOS arms only: `sans_serif()` leads with Helvetica (it led with the system font), `serif()` with Times (it led with `New York`, which does not resolve by name, so it fell to Times New Roman), `monospace()` with Courier (Menlo), `cursive` with Apple Chancery, `fantasy` with Papyrus. The family appended to every author list is the UA default (Times), not `.AppleSystemUIFont`. `system-ui`, `-apple-system` and `BlinkMacSystemFont` are unchanged.
- `rustkit-text` `map_generic` (the path without a run): `serif` is Times and `monospace` is Courier, the same faces layout measures.
- `rustkit-text` `blink_ascent`: Blink on macOS raises the ascent of Times, Helvetica and Courier by 15% of the rounded ascent + descent (`FontMetrics::AscentDescentWithHacks`). Without it a 16px line of any of the three is 16px tall; Chrome's is 18. This was already wrong for pages that name Helvetica, Times or Courier; it has to land with the change above because those three are now the default faces. Layout's shaper, `TextMetrics::from_core_text_font` and the renderer's fallback metrics all read it.

Form controls are unaffected: the UA style gives them Arial (textarea: `monospace`, so Courier now, as in Chrome).

## Tests

- `generic_families_and_the_default_are_chromes_faces` (rustkit-layout): width within 0.05px of Chrome and the PostScript name of the face, for `sans-serif`, `serif`, `monospace`, `cursive`, `fantasy`, a list with only a missing family, the initial value and the empty string.
- `times_helvetica_and_courier_get_blinks_ascent_adjustment` (rustkit-layout): an 18px line at 16px for the three families and the generics that resolve to them; Times New Roman stays 18 and Menlo stays 19.
- `test_chain_walks_past_an_uninstalled_family` (rustkit-text) now pins `monospace` to Courier, `serif` to Times, `sans-serif` to Helvetica.
- `a_face_without_a_family_name_keeps_its_own_ascent` (rustkit-text): the second commit. The real-site A/B of the first commit crashed on x.com: `blink_ascent` read `CTFont::family_name`, which panics on a face with no family name, and x's downloaded font has none. The fixture is Ahem with its `name` table hidden; the test panicked with "Fonts should always have a family name" before `family_name_or_empty`. The receipts below are all at the second commit.

Fail-first, the two new layout tests on develop __BASE__ (test code only applied):

```
generic_families_and_the_default_are_chromes_faces
thread 'text::tests::generic_families_and_the_default_are_chromes_faces' (15843714) panicked at crates/rustkit-layout/src/text.rs:3163:13:
sans-serif: 220.78125, Chrome 217.02
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 593 filtered out; finished in 0.19s
times_helvetica_and_courier_get_blinks_ascent_adjustment
thread 'text::tests::times_helvetica_and_courier_get_blinks_ascent_adjustment' (15843846) panicked at crates/rustkit-layout/src/text.rs:3215:13:
  left: 16.0
 right: 18.0
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 593 filtered out; finished in 0.23s
restored: True
```

__SUITES__

## Campaign receipt

Arms: develop **__BASE__** (this branch's base) vs fix **__HEAD__**, both release binaries built in this session with every workspace source touched first.

```
__SUMMARY__
```

__RATCHET__

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

__RUNS__

🤖 Generated with [Claude Code](https://claude.com/claude-code)
