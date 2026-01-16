Fail if any case violates:
Pixel diff > 1.00%
Style parity < 99%
Layout parity < 99%
Regression: any case worsens by >0.20% diff vs main baseline summary
Nightly gates (slow)
Run the suite at 2–3 viewport sizes (e.g. 800×600, 1280×800).
Run 3 consecutive iterations for stability.
Emit trend dashboard JSON.
Implementation surfaces:

scripts/parity_baseline.py (summary + regression logic)
Add a dedicated CI script (or reuse) that returns non-zero on gate failure.
Diagnosis strategy (turn failures into actionable fixes)
Element-attribution (highest leverage addition)
For every failing pixel diff:

Use Chrome layout-rects.json to compute which DOM rects overlap the most diff pixels.
Output Top N suspect elements + their selectors + style deltas.
This makes “38% backgrounds diff” become “background-position on .tile differs; border-radius clip missing on #card”.

Diff taxonomy
Every failure is auto-labeled into buckets using heuristics:

layout (large rect shifts)
paint_solid (uniform blocks wrong color)
paint_gradient (smooth color field mismatch)
clip_radius (diff concentrated at rounded corners)
text_metrics (diff near glyph edges/lines)
images_replaced (diff inside replaced element bounds)
Execution roadmap to reach 99%+
Phase A — Make 99% measurable (1–2 days)
Lock deterministic capture (viewport/DPR/color profile/time/disable animations).
Add font bundling + enforced font family for parity mode.
Add image normalization (alpha/colorspace).
Add element-attribution in reports.
Expected outcome: pixel diffs become stable and debuggable; text variance drops substantially.

Phase B — Fix the known worst offenders first (to unblock the parity gate)
Based on current results in the plan doc:

Built-ins about/settings: currently catastrophic diffs historically tied to background layering / paint path. Gate target: ≤1%.
Micro paint tests:
websuite/micro/backgrounds (bg-clip/size/position)
websuite/micro/gradients (angle, stop positions, interpolation)
websuite/micro/rounded-corners (elliptical radii + clip + AA)
Implementation likely touches:

crates/rustkit-engine/src/lib.rs (computed style + background parsing)
Paint/compositor crates (gradient shader math, clip masks, antialias)
Phase C — Text metrics to 99% (hardest, but necessary if “all measures” includes text-heavy cases)
Force test font + ensure identical font metrics path.
Align baseline/line-height/ascent/descent calculations.
Ensure subpixel positioning strategy matches Chrome (or quantize consistently).
Validation cases:

websuite/micro/pseudo-classes (often reveals text + inline layout issues)
Add/extend text micro-tests focused on baseline + decoration.
Phase D — Lock-in: multi-res + stability + regression budget
Run entire corpus at multiple viewports.
Enforce 3-run stability.
Add “regression budget” for PRs (must net-improve or remain within small delta; never degrade).
Deliverables (what changes land)
Parity runner becomes the one button: scripts/parity_test.py supports:
--scope builtins,websuite
--iterations 3
--viewports 800x600,1280x800
--update-baselines (manual/guarded)
--gates strict99
Oracle config pinned (chrome version + launch flags) and recorded under baselines/.
Report upgrades: top-diff elements, style/layout deltas, taxonomy label.
CI gates: per-commit fast, nightly slow.
Acceptance criteria (done means done)
For every case in built-ins + websuite corpus:
Pixel diff ≤ 1.00% with AA tolerance.
Style parity ≥ 99% on the property set.
Layout parity ≥ 99% within tolerances.
Stable across 3 runs (max-min diff ≤ 0.10%).