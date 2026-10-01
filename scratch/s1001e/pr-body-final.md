## What

Slice **S0** of `docs/SHAPED_RUN_CONTRACT_2026-09-30.md` (sections 5 and 7): `DisplayCommand::Text` carries the shaped run, and paint places that run's glyph ids from that run's face.

On develop, layout shapes every line, keeps a `Vec<f32>` of per-character advances and drops the rest. Paint then resolves the CSS `font-family` list a second time and looks every character up again. Two resolvers have to agree for the ink to match the measurement, and a cluster that is not one character has nowhere to go.

On this branch the command carries a frozen `GlyphRun`. Paint's glyph key on this command is `(face id, glyph id, size)`. The family string is not in it, and nothing on that path resolves a family list.

large-diff: about 1,270 lines. 349 are the new layout test file, 120 are renderer tests, about 230 are the new types and their doc comments, and 60 are a function body moved without change (`rasterize_char` -> `rasterize_glyph_id`).

## Change

**`rustkit-text` (commit 37ce335, no caller, no behaviour change)**
- `GlyphRasterizer::rasterize_glyph_id`: glyph N of the rasterizer's own face, with no character lookup and no fallback face. `rasterize_char` now ends there once it has found the glyph. The body is moved, not edited, so both entries return the same bitmap.
- `intern_face` / `face_font`: the fonts layout has shaped with, by face id and size. The id hashes the PostScript name, the `@font-face` file's content id (0 for a platform font), and the weight and style asked for. The last two are there because the system font's weights are instances of one variable font. The table is bounded at 1,024 fonts; when it turns over, the web-font generation is bumped so layout shapes, and records, again.
- `css_list_resolutions()`: a per-thread count of family-list resolutions, so a test can show that a paint path makes none.

**`rustkit-layout`**
- `FaceIdentity { id, postscript_name, face_index }`, `GlyphRun`, `RunGlyph`. The run carries what section 1 lists for S0: face, size, weight, style, stretch, synthesis (none), variation axes (empty), glyph id, advance, x/y offset, UTF-16 cluster range, direction, script, language, ascent, descent, leading.
- The macOS shaper records the face it selected on `ShapedRun::face`. The Windows and Linux arms set `None`; I cannot compile or run them on this seat.
- `shape_line` is the one shape call behind `shape_line_advances` (same result as before) and the new `shape_line_run`.
- `GlyphRun::freeze` applies justification, then freezes. Letter-spacing and word-spacing are already in the shaped line. It returns `None` for a run outside the slice: no named face, not left-to-right, or a character the face cannot draw (glyph 0, or an emoji).
- The emitter attaches the run to `DisplayCommand::Text`. `advances` and `ascent` are still filled, by the code that filled them before. For `text-overflow: ellipsis` the run is cut where the characters are cut (`GlyphRun::cut_with_tail`).

**`rustkit-renderer`**
- `RunGlyphKey { face, glyph_id, font_size, subpixel_phase }`, `rasterize_run_glyph`, `GlyphCache::get_or_rasterize_run_glyph` (same atlas, its own map), `Renderer::draw_glyph_run`.
- Seating is the old path's: baseline at `y + ascent` rounded to a device row, phase 0, and the bitmap rasterized at the size the old key quantizes to (`size * 10` truncated). That is what keeps the frames identical.
- If the rasterizer does not hold the run's face, `draw_glyph_run` draws nothing and returns `false`, and the command is painted by `draw_text_with_metrics`, which is not edited.

**`rustkit-engine`**: the display-list dump gains `"run": { face, glyphs, clusters }` or `null` on each `text` op.

## On today's path, by design (section 5)

- `GradientText`, `TextInput`, `Button`, `Caret`, SVG text, alt text: not touched.
- A line with a character the face cannot draw, or an emoji: no run. That is the fallback-boundary slice.
- `shape_line_advances` and `measure_text_with_spacing`: kept, same results.
- No threshold change and no baseline change.

## Acceptance (section 7)

| Check | Result |
|---|---|
| Latin projection | **Pass.** `a_latin_run_projects_onto_the_advances_layout_already_ships`: on four kerned strings (system-ui 700 at 32px, Helvetica 16px, Georgia 700 at 40px, a Georgia list at 17.6px) the run's per-cluster advances equal `shape_line_advances` to the bit, and `pen_positions` equals the character path's cursor walk exactly. Spacing and justification: `spacing_and_justification_are_frozen_into_the_run`. |
| Non-1:1 cluster | **Pass at the contract, not reachable from the macOS shaper.** `a_cluster_that_is_not_one_character_keeps_its_range`: `office` with an `ffi` ligature. The old projection returns `None`; the run has ranges `0..1, 1..4, 4..5, 5..6` and four pen positions. **The run in that test is built by hand** from a real shape of the same string, because the macOS shaper keeps ligatures off (`kerning_deltas`) and returns one glyph per character. No production line takes this path on macOS until a shaper with ligatures on fills the run. See "Not done". |
| Face identity, Windows | **Not done.** This seat cannot build Windows. The Windows shaper arms set `face: None`, so Windows keeps today's path and today's bug. The types it needs are in place. |
| Face identity, macOS | **Pass.** `the_run_names_the_face_the_family_list_resolved_to`: `Georgia, 'Times New Roman', serif` gives `Georgia`, and `Georgia-Bold` at 700, with different ids; a list that leads with an uninstalled family gives the same face and the same id. `run_glyphs_are_drawn_from_the_run_face_with_no_family_lookup`: rasterizing the run's glyphs leaves `css_list_resolutions()` unchanged, and every bitmap equals the character path's for the same letter, byte for byte. |
| Unchanged neighbours | **Pass.** The existing advance-contract, ellipsis, gradient-text and form-control tests pass unedited (rustkit-layout 591, of which 9 are new). `shape_line_advances` returns what it returned. |

Suites at head 104feb7: rustkit-layout 591/591 (9 new), rustkit-renderer 114/114 (4 new), rustkit-text 94/94. `cargo check --tests` is clean for rustkit-engine, rustkit-svg and parity-capture. The full engine suite was not run locally.

## The differential: old path against new

Arms: develop **6eb6f5f** (this branch's base) vs fix **6d8dede**. Both are release binaries built in this session, each with every workspace source touched first (the shared target judges freshness by mtime). Head 104feb7 adds one test-only commit on top of 6d8dede; no non-test code changed after the build.

The 26 campaign pages, each captured once by each binary (`--dump-frame`, plus the fix arm's display-list dump):

- **1,025 of 1,074 text commands carry a run.** The 49 without one: 45 hold an emoji and 4 a symbol the face lacks (`⏰`, `⌨`, `🆓`). Both are outside the slice by design.
- Faces the runs name: `.SFNS-Regular` 634, `.SFNS-Bold` 183, `Georgia` 59, `Menlo-Regular` 58, `Menlo-Bold` 30, `.SFNS-Semibold` 22, `.SFNS-Medium` 21, `Georgia-Bold` 13, `.SFNS-Light` 2, `Georgia-Italic` 2, `.SFNS-RegularItalic` 1.
- **25 of 26 frames are byte-identical between the arms.** On those pages the run path put every glyph where the character path puts it, with the same bitmap.
- **The one frame that differs is `css-selectors`, in one line:** "Lang prefix |= (should be italic)", 14px italic `-apple-system`, 1,294 pixels inside (35,773)-(236,787). Layout measured it in `.SFNS-RegularItalic`. The character path resolved the same list a second time and drew a different italic at those advances (upright `|`, plain `l`). The run draws the system italic, which is what Chrome's baseline shows. `diff_pct` 1.3819 -> 1.3299; Gate B paint 0.94281 -> 0.94307. This is the macOS form of the defect the contract describes: paint and layout disagreeing about the face. `italic_system_text_is_drawn_in_the_face_layout_measured` pins the new behaviour.

## Campaign receipt

```
all:      develop 2026-10-01T13:24:47 26/26 avg 1.1271 | fix 2026-10-01T13:11:00 26/26 avg 1.1251 | 25/26 identical
builtins: develop 2026-10-01T13:20:50  5/5  avg 1.9072 | fix 2026-10-01T13:06:25  5/5  avg 1.9072 | 5/5 identical
micro:    develop 2026-10-01T13:20:10 13/13 avg 0.6550 | fix 2026-10-01T13:05:43 13/13 avg 0.6550 | 13/13 identical
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): the lines that differ between the arms, develop then fix:

```
dev: css-selectors: geo_fails=3 paint=0.94281 discrete=0
fix: css-selectors: geo_fails=3 paint=0.94307 discrete=0
```

<details><summary>Per-case diff_pct (all 26), develop 6eb6f5f vs fix 6d8dede</summary>

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
| css-selectors | 1.3819 | 1.3299 | moved
| flex-positioning | 0.6327 | 0.6327 |
| form-controls | 3.2405 | 3.2405 |
| form-elements | 0.9535 | 0.9535 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-backgrounds | 1.0133 | 1.0133 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| image-gallery | 0.5217 | 0.5217 |
| images-intrinsic | 0.3267 | 0.3267 |
| new_tab | 1.5685 | 1.5685 |
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
| new_tab | 1.5685 | 1.5685 |
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| combinators | 0.6547 | 0.6547 |
| form-controls | 3.2405 | 3.2405 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| images-intrinsic | 0.3267 | 0.3267 |
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 0.4354 | 0.4354 |
| specificity | 0.6015 | 0.6015 |

</details>

## Real-site A/B

RustKit frames only, no Chrome: each site captured develop, fix, develop, fix at 1280x800 (`scratch/s1001e/s0_ab.py` on the hub). "within" is the site's own variance between two captures by the same binary; "across" is the four develop-fix pairs. The last column is from the fix arm's display list. All 20 board sites were attempted; `nan` is a capture that failed.

```
google       within develop   3.75%  within fix   3.75%  across   3.75%   0.00%   0.00%   3.75%  | 15/15 text commands carry a run (HelveticaNeue 10, ArialMT 2, Helvetica 2)
wikipedia    within develop   0.00%  within fix   0.00%  across   7.18%   7.18%   7.18%   7.18%  | 2942/2942 text commands carry a run (.SFNS-Regular 2312, .SFNS-Bold 513, .SFNS-RegularItalic 61)
x            within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  | 0/40 text commands carry a run ()
facebook     within develop   0.00%  within fix    nan%  across    nan%    nan%    nan%    nan%  | 41/43 text commands carry a run (.SFNS-Regular 32, .SFNS-Semibold 5, font00000000306ecea8 4)
shopify      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  | 8/285 text commands carry a run (ShopifyInterBeta029-Regular 6, Menlo-Regular 2)
lyft         within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  | 155/155 text commands carry a run (RebelSansTextVF-Reg 154, RebelSansDisplay-VF-Regular 1)
apple        within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  | 262/262 text commands carry a run (SFProText-Regular 186, SFProDisplay-Regular 29, SFProText-Semibold 23)
github       within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  | no display list
linkedin     within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  | 189/189 text commands carry a run (.SFNS-Semibold 136, .SFNS-Regular 53)
microsoft    within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  | 304/304 text commands carry a run (SegoeUI 304)
bing         within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  | 10/10 text commands carry a run (Tahoma 9, Tahoma-Bold 1)
yahoo        within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  | 57/57 text commands carry a run (YahooProductSans-VF 32, CentraNo2-Medium 17, CentraNo2-Bold 8)
netflix      within develop   2.33%  within fix  12.62%  across  12.66%   2.23%  12.58%   2.30%  | 106/106 text commands carry a run (font00000000306ed14e 106)
walmart      within develop  10.08%  within fix  23.03%  across  31.62%  10.08%  23.03%   0.00%  | 73/91 text commands carry a run (EverydaySansUIWeb-Regular 67, ArialMT 4, EverydaySansUIWeb-Bold 2)
squarespace  within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  | no display list
cnn          within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  | no display list
weather      within develop    nan%  within fix    nan%  across    nan%    nan%    nan%    nan%  | no display list
instagram    within develop    nan%  within fix    nan%  across    nan%    nan%    nan%   0.00%  | no display list
youtube      within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  | 0/0 text commands carry a run ()
reddit       within develop   0.00%  within fix   0.00%  across   0.00%   0.00%   0.00%   0.00%  | 0/0 text commands carry a run ()
```

- **Identical between the arms (0.00% across):** x, shopify, lyft, apple, linkedin, microsoft, bing, yahoo. google is 0.00% within each of its two page variants (Google serves two; the 3.75% is the variant, and each binary got both). youtube and reddit are blank on both arms (no text commands).
- **wikipedia changes: 7.18% of the frame**, 0.00% within each arm, so it is this change. All 2,942 of its text commands carry a run in the system font, because the page asks for `sans-serif` (finding 1 below). Scored with the board's own diff against this morning's stored Chrome frames (run `20261001T0907Z-quiet-dev7ae0e68`): **15.20% on develop, 15.15% here** against Chrome capture a; 16.92% and 16.83% against capture b. LOOKS RIGHT stays a fail on both, by the same margin.
- **netflix and walmart vary between captures by more than they differ between arms** (rotating content). netflix: 2.33% between the two develop captures, and 2.23% and 2.30% for two of the develop-fix pairs; its first fix capture is the outlier against everything, the other fix capture included. walmart: one develop-fix pair is 0.00%.
- Web-font runs paint the same pixels as the character path: lyft 155 of 155 commands in `RebelSans`, apple 262 of 262 in `SFPro*`, microsoft 304 of 304 in `SegoeUI`, yahoo 57 of 57, all 0.00% across.
- **Not measured:** github, squarespace, cnn and weather failed to capture on both arms; facebook's second fix capture and three of instagram's four failed (machine load 12 to 21). No scoring board was run; no points are claimed.

## Found by the A/B, not fixed here

1. **Layout and paint resolve the generic families differently, and layout's answer is not Chrome's.** For `font-family: sans-serif` layout's chain is `SF Pro`, the system font, Helvetica Neue, Helvetica; paint's `map_generic` says Helvetica, which is also Chrome's default. The initial `font-family` is `sans-serif` too, so the same holds for unstyled text (Chrome's default there is Times). So develop measures wikipedia in the system font and draws Helvetica at those advances. With a run, paint draws what was measured, so **wikipedia's text changes face** (7.18% of the frame differs between the arms). Against the stored Chrome frame the board's diff is 15.20% on develop and 15.15% here: neither is right. Measured in Chrome 148 on a 28-character line at 16px (`scratch/s1001e/generic.html` on the hub): unstyled 199.52 (Times), `sans-serif` 217.02 (Helvetica), `X, serif` 199.52, `X, monospace` 268.84 (Courier); layout gives 220.78, the system font, for all four. The fix is in layout's chain (`FontFamilyChain::sans_serif`, `serif`, and the fallbacks appended to every list), it changes measurements on every page that ends in a generic, and it needs its own receipt. It is the next PR from this lane. After it, this path draws Chrome's face at Chrome's advances.
2. **On some sites the selected web-font file has no glyph for ordinary letters, so their text is measured and drawn from the fallback list, one character at a time.** On shopify 277 of 285 text commands have no run, and on x 40 of 40; a Latin line without an emoji loses its run only when the face lacks a glyph. Shopify's only runs are six lines of U+00A0 in `ShopifyInterBeta029`. **The cause is not established.** The likely one: the registry picks one file per family by weight and style and ignores `unicode-range`, so a family split into subset files gets a file for another range (listed under #398's not-done). Pre-existing, and the same on both arms: those frames are identical. lyft and apple carry runs in their own faces on every line.

## Not done

- **A shaper that produces non-1:1 clusters on macOS.** S0 keeps the platform shaper (contract section 6), and that shaper is one glyph per character with ligatures off. The run can carry a ligature; nothing produces one yet. Turning ligatures on changes advances on every page that has `fi` in a face with that ligature, so it is its own slice with its own receipt.
- **Windows face identity** (above), and Linux, which has no rasterizer.
- **Language** is `None` on every run: no `lang` reaches computed style. Script is a small range table (Latin, Greek, Cyrillic, otherwise `Zzzz`).
- `advances` and `ascent` stay on the command. The contract removes them only after this differential is accepted.
- The face table keeps a web face's font alive until the table turns over (1,024 fonts).
- The full engine suite was not run locally; CI is that evidence.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
