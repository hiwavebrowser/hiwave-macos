Workstream A (highest ROI): Backgrounds + Gradients + “Image gallery”
A1) Fix multiple background layers + background-* properties
These directly hit backgrounds, gradient-backgrounds, and large parts of image-gallery.

Why: the fixtures rely on layered background: values (multiple gradients) and background-size/background-position/background-repeat.
Example fixture: hiwave-macos/websuite/cases/gradient-backgrounds/index.html uses multi-layer background: in pattern-* and background-size (e.g. .linear-6, .pattern-3).
Micro fixture: hiwave-macos/websuite/micro/backgrounds/index.html explicitly tests multiple backgrounds, position, size, repeat.
Implementation plan:
Extend the style model to represent N background layers (image/gradient/color) instead of only one background_gradient + one background_color.
Likely touchpoints:
CSS parsing currently sets only one gradient/color in hiwave-macos/crates/rustkit-engine/src/lib.rs (see background-color|background|background-image parsing).
Painting is single-layer in hiwave-macos/crates/rustkit-layout/src/lib.rs render_background().
Parse and store per-layer:
background-image (gradients + url(...))
background-size (auto/contain/cover/length pairs)
background-position (keywords + percentages)
background-repeat
background-clip / background-origin (clip already partially exists)
Render layers in correct order (bottom-most first) and apply clipping/border radius consistently.
Leverage existing background image tiling in hiwave-macos/crates/rustkit-layout/src/images.rs.
A2) Improve gradient parsing + gradient painting fidelity
Why: gradient-backgrounds and gradients rely on a broad set of gradient syntaxes.
Implementation plan:
In gradient parsing (parse_gradient / parse_linear_gradient / parse_radial_gradient in hiwave-macos/crates/rustkit-engine/src/lib.rs):
Ensure color-stop positions support % and px (and clamp/sort stops per spec).
Ensure angles/directions match CSS conventions (notably to right vs 90deg mapping).
Support multiple gradient layers (from A1).
In gradient rasterization in hiwave-macos/crates/rustkit-renderer/src/lib.rs:
Validate interpolation mode and alpha handling; the renderer already has gamma-aware interpolation—verify it matches baseline expectations for sRGB gradients.
Ensure radial gradients respect center/shape/size from the parsed model.
A3) Fix “image-gallery” by tackling aspect-ratio + background-size behavior
Why: hiwave-macos/websuite/cases/image-gallery/index.html is mostly:
CSS Grid layout
aspect-ratio
background-size: contain/cover-like behavior (simulated via gradients)
Implementation plan:
Add/verify aspect-ratio support in the style system and layout sizing.
Ensure grid item sizing respects row/column spans and intrinsic sizing.
Ensure background-size applies to gradients as well as images (comes “for free” once A1 is correct).
Workstream B (highest ROI correctness): Selectors + Combinators + Pseudo-classes
B1) Implement real combinator matching and fix :not()
Why: your selector engine currently short-circuits on combinators and has an overly-simplified :not() implementation.
Fixtures:
hiwave-macos/websuite/cases/css-selectors/index.html
hiwave-macos/websuite/micro/combinators/index.html
hiwave-macos/websuite/micro/pseudo-classes/index.html
Engine code: selector matching lives in hiwave-macos/crates/rustkit-engine/src/lib.rs (selector_matches, simple_selector_matches_with_pseudo, match_pseudo_class).
Implementation plan:
Make tokenize_selector + backward matching actually walk:
Descendant (` `)
Child (>)
Adjacent sibling (+)
General sibling (~)
Fix :not(inner) to evaluate inner as a real selector (at minimum “simple selectors”, but ideally allow the same grammar you support outside :not).
Lock in pseudo-class correctness: :first-child, :last-child, :nth-child(an+b), :nth-last-child (including negative offsets like 3n-1).
Workstream C (targeted): Sticky positioning
C1) Replace the current “treat sticky like relative” behavior
Why: layout currently treats Position::Sticky as a relative offset in hiwave-macos/crates/rustkit-layout/src/lib.rs, which won’t match Chrome.
Implementation plan:
Use the existing sticky primitives in hiwave-macos/crates/rustkit-layout/src/scroll.rs (StickyState) but integrate them into layout/paint:
Determine sticky container (nearest scroll container / viewport).
Compute stuck rect using scroll offset and container bounds.
Apply stuck rect in the render pipeline for the sticky element.
Validate against hiwave-macos/websuite/cases/sticky-scroll/index.html (header sticky + sidebars sticky + nested scroll container + overflow hidden demo).
Prioritization (balanced)
First: A1 + B1 (these should move many tests at once: backgrounds, bg-*, gradient-backgrounds, gradients, css-selectors, combinators, pseudo-classes).
Second: A3 (to crush image-gallery, which is currently a top offender).
Third: C1 (to bring sticky-scroll below threshold).
Success criteria
backgrounds, bg-solid, bg-pure show a clear reduction (expect “step change” once layering/size/position are correct).
css-selectors / combinators / pseudo-classes pass reliably (selector correctness should reduce diff dramatically).
gradient-backgrounds drops substantially once layering + size + parsing are fixed.
image-gallery drops substantially once aspect-ratio + background-size behavior is fixed.