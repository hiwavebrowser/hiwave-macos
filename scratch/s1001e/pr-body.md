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

Suites at head __HEAD__: __SUITES__

## The differential: old path against new

Arms: develop **__BASE__** (this branch's base) vs fix **6d8dede**. Both are release binaries built in this session, each with every workspace source touched first (the shared target judges freshness by mtime). Head __HEAD__ adds one test-only commit on top of 6d8dede; no non-test code changed after the build.

__RUNS__

## Campaign receipt

```
__SUMMARY__
```

__RATCHET__

<details><summary>Per-case diff_pct (all 26), develop __BASE__ vs fix 6d8dede</summary>

| case | develop | fix |
|---|---|---|
__TABLE_ALL__

</details>

<details><summary>Per-case diff_pct (builtins, micro)</summary>

| case | develop | fix |
|---|---|---|
__TABLE_REST__

</details>

## Real-site A/B

__FRAMES__

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
