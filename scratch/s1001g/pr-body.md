## What

A character the primary face has no glyph for was taken from a fixed list of families tried in order (Apple Color Emoji, Apple Symbols, Arial Unicode MS, Helvetica Neue, Menlo), in layout and in paint. Chrome asks the system for the fallback of the font in use.

`about` shows it since #417 made `<kbd>` Courier, as in Chrome. Courier has no `⌘`, `←` or `→`. Its cascade face is Menlo; the list gave Apple Symbols:

```
<kbd> box on about          Chrome   develop   fix
⌘                           21.23    22.44     21.23
←                           21.23    22.35     21.23
→                           21.23    22.63     21.23
```

## Change

- `rustkit_text::macos::fallback_face_for(primary, ch) -> Option<(CTFont, glyph id)>` is the one lookup, memoised per thread by (face, size, character):
  1. a character above Latin-1 that Apple Color Emoji has comes from it, as on develop (see the second commit below);
  2. a character on the colour-glyph path (`is_emoji`) keeps that path's faces, because that is where paint draws it from;
  3. `CTFontCreateForString` on the primary face: Core Text's cascade;
  4. the old list, as the last resort;
  5. `None` when only Core Text's LastResort face answers.
- Layout (`TextShaper::fallback_glyph_advance`) takes the advance and the extents from that face. The extents use `blink_ascent`, as the primary face's do.
- Paint (`GlyphRasterizer::rasterize_fallback`) draws the glyph id the lookup returns, so a character outside the BMP is no longer looked up as `ch as u16` on this path.

Two commits. The first put the cascade ahead of everything except `is_emoji` characters, and the campaign's geometry gate caught it: `⏰` (U+23F0) and `⌨` (U+2328) are emoji outside `is_emoji`'s ranges, the cascade gives them a text face, and their lines lost the emoji face's height. `about` went from 73 to 165 geometry failures (a feature row 54 -> 47px, a heading 29 -> 24px; Chrome 54 and 29) while every `diff_pct` stayed the same. The second commit restores Apple Color Emoji for its own characters, except ASCII and Latin-1: that face also has the digits, `#`, `*`, `©` and `®`, which are text unless a variation selector or keycap follows.

Not addressed: a text-default character that Apple Color Emoji has (`™`, `↔`, `▶`) still comes from the emoji face without a variation selector, as on develop. Chrome decides by the selector; that needs the next character in both layout and paint.

## Tests

- `a_symbol_the_face_lacks_comes_from_the_systems_cascade_face` (rustkit-text): Courier's fallback for `⌘`, `←`, `→` is Menlo, at the same size, with Menlo's glyph id.
- `the_rasteriser_draws_a_missing_symbol_from_the_cascade_face` (rustkit-text): the advance the rasteriser reports for them in Courier is Menlo's.
- `fallback_keeps_emoji_faces_and_refuses_the_last_resort_face` (rustkit-text): `☕`, `🎯`, `⏰`, `⌨` come from Apple Color Emoji; a face without digits does not take `2` from it; U+0378 is `None`; a second call gives the same answer.
- `a_symbol_courier_lacks_is_measured_in_its_cascade_face` (rustkit-layout): Courier and `monospace` measure the three symbols at Menlo's advance.
- `normal_line_height_unites_the_used_faces_like_chrome` (rustkit-layout) also pins a `⏰` line and a `⌨️` line at 26px. That pin passes on develop; it is the guard for what the first commit broke.

Fail-first, on develop __BASE__ with only the test code applied:

__FAILFIRST__

__SUITES__

## Campaign receipt

Arms: develop **__BASE__** (this branch's base) vs fix **__HEAD__**, both release binaries built in this session with every workspace source touched first.

```
__SUMMARY__
```

__RATCHET__

The seven boxes are the `<kbd>` widths above and the x of the boxes after them.

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
