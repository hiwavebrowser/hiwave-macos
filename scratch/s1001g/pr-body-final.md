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

Fail-first, on develop ac1b067 with only the test code applied:

```
== layout: a_symbol_courier_lacks_is_measured_in_its_cascade_face on develop
test a_symbol_courier_lacks_is_measured_in_its_cascade_face ... FAILED
thread 'a_symbol_courier_lacks_is_measured_in_its_cascade_face' panicked at crates/rustkit-layout/tests/fallback_face_line_height.rs:72:9:
  left: 11.25
 right: 9.6328125
test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s
restored: True
== rustkit-text: the_rasteriser_draws_a_missing_symbol_from_the_cascade_face on develop
test macos::tests::the_rasteriser_draws_a_missing_symbol_from_the_cascade_face ... FAILED
thread 'macos::tests::the_rasteriser_draws_a_missing_symbol_from_the_cascade_face' panicked at crates/rustkit-text/src/macos.rs:1806:13:
  left: 11.25
 right: 9.6328125
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 95 filtered out; finished in 0.11s
restored: True
```

11.25 is Apple Symbols' advance for `⌘` at 16px; 9.63 is Menlo's.

Suites at 00e32a1: rustkit-text 98/98, rustkit-layout 594/594 (+ `fallback_face_line_height` 4/4), rustkit-renderer 114/114. The rustkit-engine suite was not run locally; CI runs it.

## Campaign receipt

Arms: develop **ac1b067** (this branch's base) vs fix **00e32a1**, both release binaries built in this session with every workspace source touched first.

```
all:      develop 2026-10-01T18:46:40 26/26 avg 1.1224 | fix 2026-10-01T19:35:38 26/26 avg 1.1224 | 26/26 identical
builtins: develop 2026-10-01T18:42:53  5/5  avg 1.8936 | fix 2026-10-01T19:31:34  5/5  avg 1.8936 | 5/5 identical
micro:    develop 2026-10-01T18:42:18 13/13 avg 0.6548 | fix 2026-10-01T19:30:52 13/13 avg 0.6548 | 13/13 identical
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): the lines that differ between the arms, develop then fix:

```
dev: about: geo_fails=73 paint=0.93940 discrete=0
fix: about: geo_fails=66 paint=0.93940 discrete=0
```

The seven boxes are the `<kbd>` widths above and the x of the boxes after them.

<details><summary>Per-case diff_pct (all 26), develop ac1b067 vs fix 00e32a1</summary>

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
| new_tab | 1.5003 | 1.5003 |
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
| new_tab | 1.5003 | 1.5003 |
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

RustKit frames, develop / fix / develop / fix on each live URL at 1280x800 (`scratch/s1001g/ab.py` on the hub branch), then both arms scored with the board's own diff against the Chrome frames stored by today's 14:54 quiet board. "Across arms" is the share of pixels where any channel differs by more than 8.

```
site        within develop  within fix  across arms             against Chrome: develop -> fix
x                0.00%        0.00%     7.20% (all four pairs)   10.39% -> 10.08%
shopify          3.18%        0.00%     6.57% to 7.27%           18.69% / 18.99% -> 19.91%
weather          0.00%        0.00%     0.52% (all four pairs)   30.39% -> 30.53%
facebook         0.00%        0.00%     0.32% (all four pairs)   13.55% -> 13.49%
walmart          0.00%        0.00%     0.32% (all four pairs)   95.93% -> 95.93% (64.41% -> 64.40% against Chrome's second capture)
google, wikipedia, lyft, apple, microsoft: 0.00% across arms (pixel-identical)
bing             0.24%        0.24%     0.00% or 0.24%  (its own variance)
linkedin         1.55%        2.64%     0.00% to 2.96%  (its own variance; one develop/fix pair is identical)
yahoo           19.08%        0.00%     0.00% or 19.08% (the hero image arrived in one develop capture only)
netflix          0.07%        2.17%     4.94% to 5.01%           66.20% -> 66.04%
```

- **x, shopify and weather change for one reason:** the web font file selected for their text lacks ordinary letters (the `unicode-range` gap noted on #411), so every character goes through the fallback. On develop the letters came from Apple Symbols and the digits and `©` from Apple Color Emoji (the first families of the list with each glyph; checked with a probe test); now all of them come from the system's cascade face, Helvetica.
- **x: closer to Chrome, still a pass.** "Happening now." wraps to two lines and the terms line to two, as in Chrome, and the footer's year is "2026": develop drew its digits from Apple Color Emoji, spaced "2 0 2 6".
- **shopify and weather: about one point and 0.15 points worse against Chrome, both a fail on both arms.** The text is Helvetica where it was Apple Symbols; Chrome draws the site's own font. Helvetica's wider, larger glyphs cover more pixels of a layout that is misplaced on both arms.
- facebook and walmart move by 0.32% each; I did not look at where.
- **netflix's difference is the site's.** Its hero heading and button rotate between loads ("Unlimited movies, TV shows, and more" / "Plot twist: New hits, every single week"). Four more captures with the fix binary alone differ from each other by 2.18% to 4.90%, which covers the difference between the arms, and both arms score the same against Chrome.
- No board check changes on any site. github, squarespace and cnn did not capture on either arm (an earlier pass, under load) and were not retried.

The first commit alone was also captured on x, shopify and weather: the same numbers.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
