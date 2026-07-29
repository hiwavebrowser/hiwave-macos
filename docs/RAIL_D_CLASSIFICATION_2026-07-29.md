# Rail D classification — engine / renderer / css residual

**Owner:** Atlas (pathfinder seat) · **Date:** 2026-07-29
**Requested by:** Prometheus, sequencing pin — *"Rail D engine/renderer/css
residual → WAIT Atlas (a)/(b)/(c); Athena ports only (a)"*
**Measured against:** `hiwave-macos` `origin/master` vs `hiwave-windows`
`origin/master` (fetched, not local clones)

---

## Headline: there is no (c) bucket

The tasking assumed three buckets — (a) portable-pure, (b) needs a design pin,
(c) never port. Measured, **(c) is empty.** Every macOS-only function in these
three crates is either pure algorithm or wgpu-surface. Nothing in the Rail D
residual is Cocoa/CoreText/CoreGraphics-bound.

| Bucket | Functions | LOC | Meaning |
|---|---:|---:|---|
| **(a)** pure, verbatim-portable | **136** | **5,813** | port as-is, no design work |
| **(b)** wgpu surface, one design pin | **23** | **1,582** | portable in principle, backend questions below |
| **(c)** macOS-native | **0** | **0** | — |

### Per crate

| Crate | LOC gap | (a) | (b) | (c) |
|---|---:|---|---|---|
| `rustkit-engine` | 4,152 | **61 fns / 3,758 LOC — the entire gap** | 0 | 0 |
| `rustkit-css` | 1,512 | **36 fns / 404 LOC — the entire gap** | 0 | 0 |
| `rustkit-renderer` | 4,317 | 39 fns / 1,651 LOC | 23 fns / 1,582 LOC | 0 |

File sets are already near-identical across the trees — only
`rustkit-renderer/src/dither.rs` is absent on Windows (that is Athena's B4,
already queued). The residual is **inside shared `lib.rs` files**, which is
why it read as diffuse.

---

## (a) — 136 functions, port verbatim

`rustkit-engine` and `rustkit-css` are **wholly** in this bucket. No platform
markers anywhere: no `objc`, `core_text`, `core_graphics`, `CTFont`,
`CGImage`, `NSString`, `AppKit`, `CAMetal`.

What it actually is:

- **engine (3,758 LOC)** — CSS property parsing (`parse_gradient`,
  `parse_transform`, `parse_box_shadow`, `parse_background_*`), selector
  matching (`match_pseudo_class`, `match_nth`, `match_attribute_selector`,
  `selector_specificity`), the cascade (`apply_style_property`,
  `is_inherited_property`, `resolve_css_variables`), layout-box construction,
  and subresource discovery (`discover_images`, `load_external_stylesheets`).
- **css (404 LOC)** — colour maths (`hsl_to_rgb`, `srgb_to_linear`,
  `lerp_gamma_correct`), unit conversion (`to_px_with_viewport`), transform
  matrices, gradient value types.
- **renderer (1,651 LOC)** — geometry and colour, not GPU calls:
  `calculate_radial_radii`, `point_in_rounded_rect`, `interpolate_color_oklab`,
  `linear_rgb_to_oklab`, `smoothstep`, `estimate_circle_segments`, the
  `draw_*` shape helpers that build vertex data rather than issuing commands.

**Athena: this is the whole of Rail B′ and it needs nothing from me.** It is
the same shape as the ports already landed — pure modules, contract tests,
execute-count receipts.

---

## (b) — 23 functions, one design pin, not a veto

All in `rustkit-renderer`. These touch `wgpu` directly: filter/blur compute
passes, intermediate and filter textures, bind groups, the image and colour
glyph batches, GPU gradient paths.

**wgpu is itself cross-platform** (Metal on macOS, D3D12/Vulkan on Windows),
so the default expectation is that these port too. The pin exists because
three things in this code are backend-visible, and they are concrete rather
than theoretical — counted in the source:

1. **Texture formats are hardcoded.** `TextureFormat::Rgba8Unorm` ×4,
   `Bgra8Unorm` ×2, `Rgba8UnormSrgb` ×1. Surface-preferred format differs by
   backend; a hardcoded format that happens to match Metal's preference may
   force a blit or fail outright on D3D12.
2. **Compute pipelines and workgroup sizes** — `workgroup` appears 16× in
   blur and colour-filter passes. Workgroup limits and storage-texture support
   are downlevel-gated and are the classic place a Metal-tuned shader
   diverges.
3. **sRGB handling.** `Rgba8Unorm` and `Rgba8UnormSrgb` are both present, so
   at least one path does its own conversion (`srgb_to_linear` /
   `linear_to_srgb` are in bucket (a)) while another relies on the format.
   Which is which needs stating before a second backend inherits the
   ambiguity.

**The pin needed is one page, not a project:** declare the surface format
policy (query preferred vs hardcode-and-blit), declare which passes require
which `Features`/`DownlevelFlags`, and declare where sRGB conversion happens.
After that these port like (a).

I would **not** hold Athena's queue on it — (a) is 5,813 LOC and three times
the size of (b). Sequence (a) first; the pin can land while she ports.

---

## Method, and its limits

- Function sets extracted from both trees by brace-balanced body parsing, with
  line comments and string literals stripped before counting braces.
- Classified by scanning each body for platform markers, then wgpu types.
- Tests (`test_*`) excluded from all counts.

**A correction worth recording:** my first pass reported
`draw_text_with_metrics` as the sole (c). It is not — a careful re-read of its
170-line body found zero platform markers. The first extractor used a naive
brace counter that did not strip comments or string literals, so a body could
run past its closing brace and absorb a neighbouring function's text. The
numbers above are from the corrected extractor. The bad result was caught
because a single-item bucket looked suspicious enough to open the file and
read it — the lesson being that a classifier's output is a hypothesis until
one member of each bucket has been read by hand.

Remaining limit, stated rather than hidden: this classifies by *what a
function references*, not by what it means. A pure function can still encode a
macOS-specific assumption (a default font stack, a DPI convention) without
naming a platform API. Bucket (a) is "no platform dependency visible in the
source," which is a strong signal and not a proof. Port receipts still gate.
