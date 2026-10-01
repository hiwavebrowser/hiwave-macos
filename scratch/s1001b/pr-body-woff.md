## What

Atlas queue item 2 (WOFF/WOFF2), from the 2026-09-30 outside review §5. The review's premise needs one correction, and the item turned up a larger bug.

**Correction: WOFF and WOFF2 already decode on macOS.** `webfonts.rs` installs faces with `CGFont::from_data_provider`, and Core Graphics decodes both containers. Nothing in the engine filters by format. The module comment that said they "install as nothing" (what the review quoted) was out of date. So this PR adds no decompressor.

**The bug: no web font was ever painted with its own glyphs on a page loaded over HTTP**, in any format, TTF included. A document is laid out and painted once before its fonts are fetched. The renderer's glyph cache was keyed by family *name*, so that first paint stored the fallback's bitmaps under the web font's name, and every later frame reused them. Layout then measured with the web font (its cache is keyed on the registry generation) while paint drew Helvetica's glyphs at the web font's advances. The same key meant a second document declaring the same family name with a different file would reuse the first document's glyphs.

One page, three faces of Ahem (TTF, WOFF, WOFF2) plus an Arial control line, served over local HTTP, `parity-capture --url`:

| | develop e4a82f7 | this PR |
|---|---|---|
| display list advances, all three faces | 20.0 per glyph (Ahem) | 20.0 per glyph (Ahem) |
| painted glyphs, all three faces | Helvetica "X", "t", "f" spaced 20px apart | Ahem's solid em squares |

## Change

- **rustkit-renderer:** `GlyphKey` carries `web_face`: the identity of the document-registered file the run's family list resolves to, 0 for a platform font. One registry lookup per text run, not per glyph. The identity is a hash of the file's bytes, so the same file is the same cache entry however often it is installed (switching between views does not churn the atlas), and two files under one family name are two entries.
- **rustkit-text:** `webfonts::face_id(family_list, weight, italic)`, and `webfonts::inspect(bytes)`, which `install` now runs before any bytes reach Core Graphics:
  - the container is told from the bytes (sfnt, TTC, WOFF, WOFF2), never from the URL, the `format()` hint or the Content-Type;
  - sfnt: every table record lies inside the file;
  - WOFF: the length field equals the file size, every table lies inside the file and after the directory, no table is larger compressed than decoded;
  - WOFF2: the length field equals the file size, the table directory parses (`UIntBase128` with the spec's leading-zero, overflow and five-byte rules), the compressed stream lies inside the file;
  - caps: 32 MiB per file as received; 128 MiB for the decoded size a WOFF/WOFF2 directory adds up to, or its header declares.
- Ahem as WOFF and WOFF2 next to the existing TTF fixture (made from it with fontTools; 4,304 and 2,872 bytes).

**What "bounded decompression" means here, plainly.** The decompression is Core Graphics', not ours. What this PR bounds is what it is handed: the file size, and the decoded size the directory commits to. It does not validate table *contents* the way Chrome's sanitiser does. An in-engine decoder (WOFF1 is zlib per table; WOFF2 needs Brotli and the glyf/loca transform) is needed for Windows and Linux anyway and is its own item; whether it may bring a Brotli dependency is a decision for Pete (in the trench digest).

## Not in this PR

From the review's list for this item: descriptor matching beyond weight/italic, `font-display` timing, `unicode-range` subsets, and replacing the process-wide active-face slot with per-document handles. `--html-file` pages still load no relative-URL web font (no document base), which is why the 26-case campaign cannot see any of this.

## Tests

- **Engine, end to end** (`web_font_format_tests`, headless): serves the page above over HTTP, loads it with `load_url`, captures the frame, and reads the pixels of each line's five glyph cells. Each of the three formats must be more than 98% ink; the Helvetica control line must be between 2% and 60%. **Fails on develop e4a82f7** (run there with the test module and the two fixtures added, then restored; the worktree was clean after) and passes here.
- **Renderer** (`a_web_font_that_arrives_later_does_not_reuse_the_fallbacks_glyphs`): the key before the font arrives, after it arrives, after another document installs another file under the same name, and back.
- **Text, containers** (9, platform-independent): each container recognised; a file cut by 40 bytes rejected in all three; a header declaring 4 GiB decoded rejected; a WOFF table that expands past the cap, or points outside the file or into the header, rejected; a WOFF2 stream longer than the file rejected; an sfnt table past the end rejected; an oversized file rejected; `UIntBase128` edge cases.
- **Text, registry** (3, macOS): WOFF and WOFF2 install and rasterise "X" as a 20×20 block of full ink with a 20px advance (the glyphs come out of the compressed tables); a truncated WOFF2 and a 4 GiB-declaring WOFF never install; `face_id` names the file, not the family.

Suites at head fe23762: **rustkit-text 94/94, rustkit-renderer 91/91**; the two engine web-font tests pass. The full engine suite was not run locally (three lanes share the machine and the GPU test guard times out under load), so CI is the full-suite evidence.

## Campaign receipt

Arms: develop **e4a82f7** (this branch's base) vs fix **fe23762**, both release binaries built in this session from clean sources.

```
all:      develop 2026-10-01T02:48:21  26/26  avg 1.1612 | fix 2026-10-01T02:58:25  26/26  avg 1.1612 | 26/26 identical
builtins: develop 2026-10-01T02:49:33   5/5   avg 1.9072 | fix 2026-10-01T02:59:01   5/5   avg 1.9072 |  5/5 identical
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): output identical on both arms, line for line.

Identical is the expected result, not evidence the fix works: campaign pages load through `--html-file` and use no web font. The evidence for the fix is the engine test and the real-site A/B below.

<details><summary>Per-case diff_pct (all 26), develop e4a82f7 vs fix fe23762</summary>

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
| rounded-corners | 1.3221 | 1.3221 |
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| specificity | 0.6015 | 0.6015 |
| sticky-scroll | 0.6575 | 0.6575 |

</details>

## Real-site A/B

AB_PLACEHOLDER

🤖 Generated with [Claude Code](https://claude.com/claude-code)
