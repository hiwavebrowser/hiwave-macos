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

Fail-first, the two new layout tests on develop a0583fa (test code only applied):

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

Suites at 5ad75d9: rustkit-layout 594/594 (2 new), rustkit-text 95/95 (1 new, 1 updated), rustkit-css 47/47, rustkit-renderer 114/114, rustkit-svg 26/26.

**The rustkit-engine suite was not run to completion locally.** Two attempts at machine load 15 to 35 (other lanes building): every failure was the suite's own GPU test guard timing out (`GPU test guard: waited 120s`, `lib.rs:105`), not an assertion, so the run says nothing about this change. CI is the first full run of it; if an engine test pins the old default face, it will show there and I will fix it on this branch.

Geometry gate (CI's Gate A, run locally through `ratchet_local.py`): the ratchet holds on both arms, and three lines differ. They are all `<kbd>` boxes, which are `font-family: monospace` and so change from Menlo to Courier:
- Chrome's baseline confirms Courier there: `<kbd>Ctrl</kbd>` on `new_tab` is 28.81px of text at 12px bold (7.20 per character; Menlo is 7.22), in a 14px line.
- `about` 66 -> 73 failing boxes: three `<kbd>` holding a symbol Courier has no glyph for (`⌘`, `←`, `→`) and four boxes to their right. Chrome's fallback face for them has Menlo's advance (21.23px box); RustKit's fallback list tries Apple Symbols first (22.3 to 22.6px). With Menlo as the primary face the symbol was in the font, which hid this. The fix is for glyph fallback to ask Core Text for the cascade face of the primary font; it is not in this PR.
- `new_tab` 15 -> 17: the three `<kbd>` of the third shortcut sit 5px high (y 419.5, Chrome 424.5). The second and eighth shortcuts already fail the same way on develop; Menlo's wider advance hid it on the third. `new_tab`'s pixel diff improves (1.5685 -> 1.5003).

## Campaign receipt

Arms: develop **a0583fa** (this branch's base) vs fix **5ad75d9**, both release binaries built in this session with every workspace source touched first.

```
all:      develop 2026-10-01T16:14:01 26/26 avg 1.1251 | fix 2026-10-01T17:07:21 26/26 avg 1.1224 | 24/26 identical
builtins: develop 2026-10-01T16:06:33  5/5  avg 1.9072 | fix 2026-10-01T17:03:52  5/5  avg 1.8936 | 4/5 identical
micro:    develop 2026-10-01T16:05:57 13/13 avg 0.6550 | fix 2026-10-01T17:03:08 13/13 avg 0.6548 | 12/13 identical
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): the lines that differ between the arms, develop then fix:

```
dev: about: geo_fails=66 paint=0.93940 discrete=0
fix: about: geo_fails=73 paint=0.93940 discrete=0
dev: form-controls: geo_fails=43 paint=0.95571 discrete=0
fix: form-controls: geo_fails=43 paint=0.95576 discrete=0
dev: new_tab: geo_fails=15 paint=0.94543 discrete=0
fix: new_tab: geo_fails=17 paint=0.94520 discrete=0
```

<details><summary>Per-case diff_pct (all 26), develop a0583fa vs fix 5ad75d9</summary>

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
| form-controls | 3.2405 | 3.2388 | moved
| form-elements | 0.9535 | 0.9535 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-backgrounds | 1.0133 | 1.0133 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| image-gallery | 0.5217 | 0.5217 |
| images-intrinsic | 0.3267 | 0.3267 |
| new_tab | 1.5685 | 1.5003 | moved
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
| new_tab | 1.5685 | 1.5003 | moved
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| combinators | 0.6547 | 0.6547 |
| form-controls | 3.2405 | 3.2388 | moved
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

RustKit-only A/B on live URLs, develop a0583fa vs fix 5ad75d9, each captured A, B, A, B at 1280x800 (machine load 12 to 23, so no Chrome oracle run; `nan` is a capture that did not finish in 150 s). "within" is the same binary twice (the site's own variance), "across" is the four develop/fix pairs:

```
x            within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  | 0/40 text commands carry a run ()
wikipedia    within develop   0.00%  within fix   0.00%  across  12.30%  12.30%  12.30%  12.30%  | 2938/2938 text commands carry a run (Helvetica 2308, Helvetica-Bold 513, Helvetica-Oblique 61)
google       within develop   8.87%  within fix   8.36%  across   0.74%   8.49%   8.81%   3.76%  | 15/15 text commands carry a run (HelveticaNeue 10, ArialMT 2, Helvetica 2)
facebook     within develop   0.00%  within fix   0.00%  across   1.05%   1.05%   1.05%   1.05%  | 41/43 text commands carry a run (.SFNS-Regular 32, Times-Bold 5, font00000000306f027b 4)
netflix      within develop   4.72%  within fix   4.78%  across   2.16%   4.58%   4.91%   4.69%  | 104/104 text commands carry a run (font00000000306f02e2 104)
lyft         within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  | 155/155 text commands carry a run (RebelSansTextVF-Reg 154, RebelSansDisplay-VF-Regular 1)
shopify      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  | 8/285 text commands carry a run (ShopifyInterBeta029-Regular 6, Menlo-Regular 2)
apple        within develop   0.00%  within fix    nan%  across    nan%   0.00%    nan%   0.00%  | 262/262 text commands carry a run (SFProText-Regular 186, SFProDisplay-Regular 29, SFProText-Semibold 23)
```

- **wikipedia** is the one site whose frame follows the binary (12.30% of pixels, 0.00% within each arm): its 2938 text commands go from the system font to Helvetica, Helvetica-Bold and Helvetica-Oblique, which is what Chrome draws.
- **facebook** moves 1.05%: five text commands with no author font are now Times-Bold.
- **x**, **lyft** and **shopify** are pixel-identical across arms. google and netflix differ within an arm by as much as across (rotating content).
- **x crashed on the first commit** (35b90b8) and loads on the head: see the third test above.

The same frames against the Chrome captures stored by the last quiet board (`20261001T1832Z-quiet-dev2b764be`, the oracle's own `diff`), develop -> fix:

| site | Chrome capture | develop | fix |
|---|---|---|---|
| wikipedia | b | 15.15% | 15.28% |
| wikipedia | a (the unstable capture: Chrome-vs-Chrome was 42.5% in that run) | 44.24% | 44.22% |
| facebook | a and b | 13.63% | 13.55% |
| x | a | 10.39% | 10.39% |

No board check changes. wikipedia does not move toward Chrome on the pixel meter even though its face is now Chrome's: the page's diff is its layout (no sidebar, no banner, a different column), and the text sits in different places in the two browsers either way. The claim this PR makes for real sites is the face and its metrics, measured on the fixture above, not a board point.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
