# Outside-eye R1 — hiwave-macos PR #86 (scaled gradient must clip to box)

**Date:** 2026-08-04  
**Seat:** Prometheus (design / research only)  
**Tip:** `f7d89182bf1ea368d26eb068812caab8ebe768ce`  
**Branch:** `fix/gradient-paint-escapes-box`  
**Base at open:** `962efc14` (#85 merge)  
**Master at review:** `d5df733` (#87 parity plan + #82/#83 pair landed)  
**Scope:** `crates/rustkit-layout/src/lib.rs` only (+104 / −0)  
**CI:** audit + pr-swarm×4 + pr-aggregate **SUCCESS**  
**Mergeable:** true  

## Ruling

| Item | Ruling |
|------|--------|
| #86 product | **DESIGN CLEAR / APPROVE merge** @ `f7d8918` |
| Mechanism (PushClip when positioned_rect exceeds container) | **CLEAR** |
| CPU consumer path (`draw_solid_rect_f32` ∩ `current_clip`) | **CONFIRMED load-bearing** |
| Stated residual: square corner notches under border-radius | **ACCEPT** (~0.05% vs prior 4× bleed) |
| Stated residual: GPU gradient queue ignores clips | **ACCEPT** (off by default; `RUSTKIT_GPU_GRADIENTS=1`) |
| Stale NoRepeat comment ("viewport/wgpu will clip") | **SOFT nit** — comment lies after fix; scrub optional same PR or follow-up |
| Tip behind master (#82/#83/#87) | **SOFT** — merge-tree **0 conflict markers**; rebase optional |
| Windows SAME_DEFECT | **NO** — Windows has no multi-layer `render_background_layer` / `background_layers` gradient path; port note when that surface lands |
| Merge | **Atlas** — **not Prometheus** |

## Independent ground

### Worktrees
- Tip: `/tmp/hiwave-pr86-r1` @ `f7d8918`
- Master: `/tmp/hiwave-pr86-master` @ `d5df733`

### Master defect (CONFIRMED)
In `DisplayList::render_background_layer` → `BackgroundImage::Gradient`:
1. `calculate_background_rect` with `BackgroundSize::Explicit { width: Some(-400.0), … }` expands to **4×** container (`v < 0` ⇒ `container * (-v/100)`).
2. NoRepeat arm calls `render_gradient(gradient, positioned_rect, border_radius)` **without** a container clip.
3. Stale comment claimed viewport/wgpu would clip; viewport clip = viewport bounds, so bleed reaches neighbouring cards (Pete: "-45deg Rainbow" on gradient-backgrounds; ~14.44% of that parity case).
4. Geometry was never wrong — only paint escaped.

Master chunk at review: **no** `positioned_rect.x < container.x` predicate; **no** PushClip in the layer arm.

### Tip arm
```text
needs_clip = positioned exceeds container on any edge
if needs_clip { PushClip(container) }
match repeat { NoRepeat | Repeat* | Space|Round → render_gradient(...) }
if needs_clip { PopClip }
```
One clip wraps every repeat arm — closes oversized NoRepeat **and** tile-edge overpaint on Repeat.

Outer `background_clip` PushClip (non-BorderBox only) is a **different** `needs_clip` in the solid/layer wrapper; scopes do not collide.

### CPU path is real (not display-list theatre)
- `draw_linear_gradient` itself never reads `current_clip`.
- All CPU strips/cells call `draw_solid_rect_f32`, which **does**:
  `rect = rect.intersect(current_clip())` or early return.
- `PushClip` / `PopClip` update `clip_stack` with intersect semantics.
- Therefore DL PushClip is sufficient for the default CPU gradient path.

### GPU residual (stated, verified)
When `gpu_gradients_enabled`, `draw_linear_gradient` queues `QueuedLinearGradient { rect, … }` and returns **before** CPU cell draw — queue does not snapshot clip. Off by default. ACCEPT as residual unit, not a block.

### Local tests
```text
test_oversized_gradient_paint_is_clipped_to_its_box ... ok
test_normal_size_gradient_pushes_no_clip ... ok
```
Control test prevents accidental unconditional clip (would mask predicate regressions).

### merge-tree
`git merge-tree` tip ↔ `origin/master`: **0** `<<<<<<<` markers. Master moved on #82/#83/#87 (layout flex + docs + form transparent); product file for #86 is layout DisplayList only — clean land.

### Cross-port
| Surface | Multi-layer gradient + size % | Clip on oversized |
|---------|-------------------------------|-------------------|
| macOS tip | YES | YES (this PR) |
| macOS master | YES | **NO** (defect) |
| Windows | **NO** `render_background_layer` | N/A until port |
| Linux | not re-probed this tick | bank as port note if present later |

## Non-actions
- No merge / force-push / spend / master write from Prometheus.
- Do not re-open #85 CLEAR, #82+#83 pair CLEAR, P0a-0 element_id amend, dual-oracle finish-line pin unless tip/measurement moves.
- Do not treat GPU residual or radius notches as blockers for this unit.

## Next
- **Atlas:** land #86 when process allows (optional rebase onto master for linear history).
- **Optional scrub:** delete or rewrite NoRepeat "viewport/wgpu will clip" comment.
- **Prometheus next:** outside-eye first *new* tip (#88 only if residual beyond banked P0a-0 amend · Community #2 · B3-paint · Win tip move). Else STOP.

— Prometheus · outside-eye R1 · 2026-08-04
