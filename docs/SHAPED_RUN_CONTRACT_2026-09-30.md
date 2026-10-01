# Shaped-run contract — text and font transport

> **Status:** Proposed design pin. Pete reviews architecture. Atlas reviews lane fit.
> **Source:** Atlas #567, Pete-approved guidance `HiWave-Chrome-Parity-Guidance-2026-09-30` §5.
> **Anchors:** checked on `develop` `b946849` (2026-10-01). Atlas verified the same claims at `af4b95d`, an ancestor of this tip; line numbers below are the current ones.
> **Scope:** one contract and one first slice. This pin does not change `docs/PARITY_FINISH_LINE_PLAN_2026-08-04.md`, does not move a threshold, and does not authorize an engine edit by itself.
> **Companion:** `docs/LAYOUT_CONSTRAINTS_FRAGMENTS_2026-09-30.md` (guidance §6). The two slices ship independently.

## 0. Decision

Layout, intrinsic measure, line breaking, paint, and caret consume one immutable shaped-run result. Paint places glyphs from that result. Paint does not resolve a CSS `font-family` list.

The advance contract (`7ab0d51`, comment on `DisplayCommand::Text` at `crates/rustkit-layout/src/lib.rs:5977`) already made layout's advances the placement source and deleted a third paint-side shaper. That step stands. It is not the contract: the value that leaves layout is a `Vec<f32>` of per-character advances, and it is dropped whenever a cluster is not one character.

## 1. The result

An immutable value, produced by one shaper call, frozen before it crosses into a display command. Letter-spacing, word-spacing, and justification slack are applied before the freeze. Paint does not edit advances after that.

| Field | What it carries |
|---|---|
| Face identity | The face actually selected: PostScript name (Core Text), or DirectWrite family plus face index. Plus face index in the file, the size in CSS px, the variation axis values (empty until a variable-font slice), and the synthesis decision (faux bold / faux italic, or none). |
| Glyphs | Glyph id in that face, advance, x/y offset from the pen, and a source cluster range. |
| Cluster range | UTF-16 code-unit range into the source string. Several glyphs may share one range (a ligature, a combining sequence). A range is the only legal break and caret boundary. |
| Run boundaries | Script, language, direction, font-feature settings, and the fallback-run split. One run is one face. A character drawn from another face is the next run, not a `.notdef` advance wearing the primary face's name. |
| Line metrics | Ascent, descent, leading (half-leading already resolved the way the line box needs), and the logical-to-visual map caret and selection walk. |

`text::ShapedRun` (`crates/rustkit-layout/src/text.rs:334`) is the nearest object and is not this contract. It stores a family **string**, one `char` per glyph (`PositionedGlyph` at `text.rs:317`), a single `cluster: u32`, and a direction defaulted to LTR. It has no face id, no variation, no synthesis flag, no script, no feature list, and no logical-to-visual map. Widening that struct in place, then still projecting it down to `Vec<f32>`, does not implement the pin.

## 2. Consumers

Every consumer reads the same run. They do not reshape.

| Consumer | Reads |
|---|---|
| Layout | Advances, offsets, line metrics. Breaks only on a cluster boundary, then reshapes the broken slice through the same shaper. |
| Intrinsic measure | The same advances and line metrics, under the same face, for min-content and max-content. |
| Line breaking | Cluster boundaries plus advances. A break that is not a cluster boundary is a bug in the breaker, not a special case in paint. |
| Paint | Glyph ids, offsets, advances, face id, size. The CSS family list is not an input. |
| Caret and selection | Cluster ranges and the logical-to-visual map. A caret offset is a cluster boundary, not a `char` index. |

## 3. What develop does today

Confirmed at `b946849`.

**Layout throws the run away.** `shape_line_advances` (`lib.rs:8389`) shapes, applies spacing, then returns `Some(run.glyphs.iter().map(|g| g.advance))`. At `lib.rs:8419` it returns `None` when `run.glyphs.len() != text.chars().count()`. The comment at `lib.rs:8385` states the consequence: the renderer then uses its own advances. Ligatures, combining marks, and any cluster that is not one code point never reach paint.

The block/inline path emits that vector. `DisplayCommand::Text` (`lib.rs:5967`) carries `text: String`, `font_family: String`, `advances: Option<Vec<f32>>`, and `ascent: Option<f32>`. The emitter at `lib.rs:7814` copies `style.font_family.clone()` (`lib.rs:7820`) — the author's list, not the face the shaper selected — and attaches the advance vector. Justification (`lib.rs:7741`) mutates that vector in place, zipped with `text.chars()`.

**Paint walks characters.** `draw_text_with_metrics` (`crates/rustkit-renderer/src/lib.rs:4993`) iterates `text.chars()` at `lib.rs:5047`. The glyph key is `(codepoint, font_family string, size, weight, style)` (`lib.rs:5048`). `subpixel_phase` is frozen at 0 (`lib.rs:5049`). A missing layout advance falls back to the rasterizer's own advance (`lib.rs:5080` on the color-glyph arm; the grayscale arm does the same). Gradient text has a second character loop at `lib.rs:4854`. Form controls measure with a separate `run_shaper` (`lib.rs:4956`) that takes a family string.

**Kerning is real and deliberately one-unit.** `kerning_deltas` (`text.rs:1344`) builds a Core Text line with `kCTLigatureAttributeName = 0` (`text.rs:1372`) so glyphs stay one per UTF-16 unit. The comment at `text.rs:1338` says so. It fixes Latin pair kerning (the "CSS Specificity Test" drift in that comment). It is not a shaping model for ligatures, marks, contextual forms, mixed script, or emoji sequences.

**Windows paint re-resolves the raw list.** Layout on Windows walks `FontFamilyChain::all_families()` (`text.rs:846`), which `from_css_value` (`text.rs:179`) split on commas. `Georgia, 'Times New Roman', serif` can select Georgia. The display command still ships the raw list (`lib.rs:7820`). DirectWrite paint then calls `FindFamilyName` on that entire string (`crates/rustkit-renderer/src/glyph.rs:646`, inside `rasterize_glyph_directwrite` at `glyph.rs:620`). A list is not an installed family, so the lookup fails and the ladder is Segoe UI, Arial, Tahoma (`glyph.rs:658`). That is a sans-serif face painted for a Georgia measurement. The Windows `ShapedRun` also stores `font_chain.primary` (`text.rs:1036`), not the family the loop actually bound (`text.rs:857`).

macOS paint is a different defect of the same shape. `create_font` (`crates/rustkit-text/src/macos.rs:447`) does split a CSS list, so a Georgia list can paint Georgia on macOS. Paint still builds the key from the CSS string (`renderer/src/lib.rs:5048`) and rasterizes one character at a time. Weight, a document face, and fallback can diverge from the face layout measured. The contract is the same on both seats: paint receives a face id.

**Linux has no rasterizer to bind a face id to.** `rustkit-text` outside macOS and Windows returns `TextBackendError::NotImplemented` (`crates/rustkit-text/src/lib.rs:117`, `mod nowin` at `lib.rs:105`). The renderer draws a bordered box (`glyph.rs:439`).

## 4. Platform boundary

One shaping contract. Three platform edges, used for discovery and rasterization only.

| Edge | Seat | Today |
|---|---|---|
| Core Text | macOS | `rustkit-text/src/macos.rs` (`create_font`, `GlyphRasterizer`). Shaping in `rustkit-layout/src/text.rs` via `CTFontGetGlyphsForCharacters` plus the ligature-off kerning line. |
| DirectWrite | Windows | `rustkit-text/src/win.rs` for collection and face lookup. Layout shaping in `text.rs:816`. Raster in `glyph.rs:620`. |
| FreeType | Linux | Named here so the Linux seat has an edge to implement. Not in the tree. The `nowin` stub and the bordered-box placeholder stay until that seat lands. |

The same font bytes and the same shape inputs must produce comparable glyph ids, cluster ranges, and advances on each implementation of the contract. Raster pixels are compared per OS against Chrome on that OS (`MISSION.md` platforms: macOS is the reference tree). Identical cross-OS font pixels are not the bar.

Web fonts stay beside this slice. `webfonts.rs:19` accepts TTF/OTF only; WOFF/WOFF2 install as nothing. The non-macOS registry (`webfonts.rs:164`) installs nothing. The registry is a process-wide slot swapped per view (`webfonts.rs:9`). Completing fetch, decode, descriptor match, and cache invalidation is a later text slice. S0 shapes whatever face the current resolver already returns.

## 5. First slice — S0, `DisplayCommand::Text`

**Name:** S0, the horizontal single-face `DisplayCommand::Text` run.

**What moves:** the emitter at `lib.rs:7814` and `draw_text_with_metrics` (`renderer/src/lib.rs:4993`). The command carries the frozen run from §1. Paint places that run's glyph ids through the platform rasterizer for the run's face id. The CSS family string is not part of the glyph key on this command.

**What S0 is required to populate:** face id, size, weight, style, stretch, synthesis (none, unless the current platform shaper already synthesized), glyph id, advance, x/y offset, UTF-16 cluster range, direction, and line metrics. Variation axes are an empty list. Script and language are recorded; for this slice the executed corpus is a single LTR run in one face, including clusters where glyph count differs from character count (the `lib.rs:8419` early-return). A character the face cannot draw ends the run; S0 does not invent a fallback face. Those characters stay on today's path until the fallback-boundary slice.

**What keeps today's path, so other lanes keep shipping:**

- `GradientText` (`lib.rs:6175`, paint loop at `renderer/src/lib.rs:4854`), `TextInput`, `Button`, and `Caret` keep `advances` / family-string paint.
- `shape_line_advances` and `measure_text_with_spacing` stay. Flex, grid, forms, corners, and web-font work keep calling them.
- `advances` and `ascent` remain on `DisplayCommand::Text` until the differential in §7 is green. S0 fills them by projecting the run, so a caller that still reads the vector keeps compiling. After the differential is green, those two fields on this one command are that projection. They are not a second shaper.
- A lane that edits the `DisplayCommand::Text` emitter preserves the run field. A lane that does not touch that command is unaffected.
- No parity threshold change and no baseline regeneration in the engine PR that implements S0. Instrument changes stay in their own PR, per the finish-line plan.

S0 is the glyph-run display command for one text path. It is not a rewrite of line breaking, caret, emoji sequences, RTL, or web fonts.

## 6. HarfRust and the independence policy

`MISSION.md` pillar 1: RustKit renders every page. No Chromium, no WebKit, no system webview underneath. Pillar 2: MPL-2.0, and no Chromium dependency. The same document already depends on a Rust library for a bounded job (Flow Shield uses Brave's `adblock-rust`). A Rust shaping library is in that category. Chromium is not.

[Chromium's `harfbuzz_shaper.cc`](https://raw.githubusercontent.com/chromium/chromium/main/third_party/blink/renderer/platform/fonts/shaping/harfbuzz_shaper.cc) is evidence that a browser wants a shaping interface with glyph ids, advances, offsets, and clusters. It is not source to import, and it is not a reason to take a Chromium tree.

[HarfRust](https://github.com/harfbuzz/harfrust) is a candidate to evaluate, not a dependency of S0. S0 keeps the platform shaper that already runs (Core Text on macOS, DirectWrite glyph indices on Windows) and stops discarding its result. Adopting HarfRust would not implement fallback, line breaking, font loading, or raster matching. Those stay HiWave's.

Evaluation, parallel to S0 and not a gate on it:

1. License check against MPL-2.0, and a check that the crate vendors no Chromium source.
2. The same frozen corpus and the same font bytes through HarfRust and through the in-house/platform shaper. Compare glyph ids, cluster ranges, and advances.
3. The platform edge in §4 stays the raster and discovery boundary either way.

If the evaluation fails, or if shaping stays in-house, the corpus and the §1 contract are still required. The choice is which code fills the run. It is not whether the run exists.

## 7. Acceptance for S0

Differential against the old path, on the current Latin advance-contract cases plus three new checks. Existing matching Latin cases stay green.

| Check | Pass |
|---|---|
| Latin projection | On the kerning corpus `kerning_deltas` already names (pair kerning, one glyph per unit), the run's per-cluster advances equal today's `shape_line_advances` vector, and painted pen positions follow the run. |
| Non-1:1 cluster | A run the old function rejects at `lib.rs:8419` (glyph count ≠ char count) produces cluster ranges. The differential records the old `None`. Paint of S0 places the run's glyphs. It does not fall through to per-character raster advances. |
| Face identity, Windows | `font-family: Georgia, 'Times New Roman', serif`. The run's face id is Georgia (the family layout already walks at `text.rs:846`). The DirectWrite rasterizer is invoked with that id. A trace that shows Segoe UI, Arial, or Tahoma fails. This is the regression the screenshot supports. |
| Face identity, macOS | The same declaration records the face `create_font` would select, and the S0 paint key is that face id. A second `create_font` on the CSS list in the S0 paint path fails the test. |
| Unchanged neighbors | `GradientText`, form controls, and today's `shape_line_advances` callers produce the same advances they produce on `b946849` for the existing Latin tests. |

Caret, Arabic joining, Indic clusters, emoji sequences, CJK fallback, mixed bidi, and variable axes are the contract's later corpus (§2 and guidance §5). They are not S0's exit bar. S0's exit bar is the table above, with the old path still available as the differential oracle.

## 8. After S0, in order

Each of these is its own slice, with the old path kept as the differential until that slice's receipt is green.

1. Fallback-run boundaries on `DisplayCommand::Text` (the `.notdef` / emoji split already special-cased in `text.rs:1228` and `text.rs:887`).
2. `GradientText` onto the same run.
3. Line breaking consumes cluster ranges (today's breaker plus `FIT_EPSILON` at `text.rs:1808` stay until then).
4. Caret and selection walk the logical-to-visual map. Form-control text (`TextInput`, `Button`) moves here, not before, because caret geometry depends on clusters.
5. Web-font completeness (WOFF/WOFF2, non-macOS registry, document-scoped face handles). The §1 face id is what that slice invalidates.
6. The HarfRust-versus-in-house evaluation from §6, if it has not already closed.

Flex, grid, forms semantics, corners, and paint-geometry lanes do not wait on any of these. They keep calling the measurement functions S0 leaves in place.
