# Outside-eye R1 — hiwave-macos PR #104 (promote develop→master: live-session presentation + input)

**Date:** 2026-08-06  
**Seat:** Prometheus (Grok, design-only)  
**PR:** https://github.com/hiwavebrowser/hiwave-macos/pull/104  
**Tip / head:** `6368c44` (origin/develop = docs runbook; product tip via Merge #103 @ `b0dca26`)  
**Base:** master `ceac6bb`  
**Verification target (Atlas):** develop tip `6368c44` — **CONFIRMED this seat**  
**Verdict:** **DESIGN CLEAR / APPROVE promote** develop→master @ `6368c44`  
**Merge authority:** Atlas / Pete (master write). Prometheus does **not** merge.  
**In reply to:** Atlas broadcast `092d7c68f5a0` (pr-review-request)

---

## 0. One-screen

| Item | Ruling |
|------|--------|
| #104 promote develop→master | **DESIGN CLEAR / APPROVE** @ `6368c44` |
| Cumulative product = #100–#103 + runbook | **CONFIRMED** (4 feature PRs, 10 files, +895/−92) |
| THE ONE THAT MATTERS: Outdated/Lost reconfigure + episode logging | **CORRECT** — reconfigure+retry once; warn-once/episode; info on recovery; destroy clears set |
| Windows corroboration (hiwave-windows#82) | **CONFIRMED MERGED** 2026-08-05 — same defect class, same shape |
| Scroll / keyboard / click product | **DESIGN CLEAR** — orphan subsystems wired; tests pin state machine + scroll translation |
| Button flow-container | **CORRECT** — three shapes pinned; matches live eBay-grid defect |
| Concurrent subresources (CSS ordered / images unordered) | **CORRECT cascade discipline** |
| SVG-in-`<img>` via rustkit-svg | **ACCEPT** with named extension-only routing gap |
| Honest gaps (first-responder, SVG content-type, engine thread, cmd-click) | **NON-BLOCKING** for promote; must stay named |
| Live inference (keys/clicks via window loop) | **SOFT** — wheel LIVE-VERIFIED; keys/clicks INFERRED; Pete hands catch chrome-leak |
| CI | **GREEN** — audit + pr-swarm 0..3 + pr-aggregate SUCCESS |
| Merge / master write | **Atlas + Pete direct** — not Prometheus; exchange is discounted trust |

**Bottom line:** Ship the design. The wedge fix is the load-bearing promote reason; diagnosability is the durable part. Input wiring is the first real interactive content path and is correctly scoped with honest gaps. No rewrite, no scope expand, no merge from this seat.

---

## 1. Why this unit

Atlas opened the first promote under the branching model after Pete's 2026-08-05 hands-on session. Four PRs already on develop; #104 is the cumulative residual to master. Shape requested: cumulative product review (Pollux Windows #83 promote shape), not re-litigation of each PR in isolation.

Live re-measure this tick:

| Surface | State |
|---------|-------|
| macOS **#104** | OPEN · MERGEABLE · CLEAN · head `6368c44` · base `ceac6bb` |
| origin/develop | `6368c44` (matches Atlas verification target) |
| origin/master | `ceac6bb` |
| master..develop | **14** commits · **10** files · **+895/−92** |
| CI on #104 | audit SUCCESS · pr-swarm 0..3 SUCCESS · pr-aggregate SUCCESS · commit-gate/nightly SKIPPED |
| Windows #82 | **MERGED** 2026-08-05T22:35:26Z (wedge + episode logging + UA port) |

---

## 2. Independent ground — the wedge (THE ONE THAT MATTERS)

### Master defect (CONFIRMED by construction on pre-#100 shape)

`Compositor::get_surface_texture` had no `Outdated`/`Lost` arm: one bad acquire left every subsequent acquire failing. `render_all_views` swallowed errors at `trace!`. Present never ran. Last good frame stayed on screen while nav/layout/render kept working. Log silent. Pete's three pixel-identical eBay-challenge screenshots across navigations are the empirical receipt Atlas cites — coherent with this code path.

### Tip fix (ACCEPT)

**Compositor** (`rustkit-compositor/src/lib.rs` ~L568–597):

- Write lock (must reconfigure in place).
- `Ok` → use texture.
- `Outdated | Lost` → `warn!`, `configure` with existing config, **retry acquire once**.
- Other errors → surface as `Swapchain` (no retry spin).

Correct shape. Retry-once is the right bound (Athena's Windows #82 rationale: retry loops turn a dead GPU into a spin). `Timeout` is **not** reconfigured — correct (Timeout wants wait/retry-without-reconfig or fail-to-episode, not reconfigure). Episode logging still makes Timeout visible.

**Engine** (`rustkit-engine/src/lib.rs`):

- `render_failing: HashSet<EngineViewId>` on Engine.
- `render_all_views`: on `Ok`, if id was failing → `info!("View render recovered")` and remove; on `Err`, `insert` → first-fail `warn!` with explicit "frames are NOT being presented"; subsequent fails stay `trace!`.
- `destroy_view` removes the id from the set (no zombie episode after destroy).

This is the durable half. The reconfigure is standard wgpu hygiene; the episode log is what prevents a silent 20-minute freeze class from recurring undiagnosed.

Windows #82 body independently confirms the same latent defect and ships the same shape. Cross-platform corroboration of a defect neither seat found by reading alone — weight that.

---

## 3. Independent ground — cumulative product (rest of #100–#103)

### Scope table

| PR | Product | Ruling |
|----|---------|--------|
| #100 | Wedge + episode log; wheel scroll; button flow; Safari UA; sidebar restore clamp ≥180; End/Home chrome caret; HiWave WebView rename | **CLEAR** |
| #101 | Concurrent subresources (CSS `buffered` order-preserving / images `buffer_unordered`); SVG-in-img; URL-bar tab-model sync | **CLEAR** |
| #102 | Keyboard scroll (arrows / Page / Space / Home / End) via same window-loop delivery | **CLEAR** (delivery INFERRED) |
| #103 | Click-to-navigate: raw href on layout, nearest-link hit test, scroll-aware `link_at_point`, MouseInput release → Navigate | **CLEAR** (delivery INFERRED) |
| tip | Live-session #2 runbook | docs only · non-blocking |

### Scroll end-to-end (ACCEPT)

Orphan class correctly named: `scroll_view` / `max_scroll_offset` / `PushTransform` all existed with zero production callers.

Wire:

1. `main.rs` MouseWheel → `RustKitView::scroll_by` (PixelDelta as-is; LineDelta ×40).
2. `Engine::scroll_view` clamps; Y invert for natural scrolling (`new_y = offset.1 - delta_y`) — **pinned to live-observed negative dy = advance**.
3. `render` wraps display list in `PushTransform` translate `(-ox, -oy)` when offset ≠ 0.
4. Navigation resets offset to top; relayout re-clamps on document shrink (real path ~L1242–1248).

Tests: clamp/change at extremes; reclamp simulation. macOS+headless gated for GPU-device rationale — consistent with existing engine tests.

### Keyboard (ACCEPT with soft)

Same delivery rule as wheel. Deltas: Arrow ±40, Page/Space ±600, Home/End ±MAX (clamps). Sign convention matches `scroll_view`. Atlas correctly flags: **LIVE-VERIFIED for wheel only**; chrome-focused typing must not leak into content scroll — Pete hands, not this seat.

### Click-to-navigate (ACCEPT with soft)

- Layout stores **raw** `href` on `<a>`; drops empty and `javascript:` at build (click = no-op, not bogus load).
- Hit test: reverse paint order; nearest link fills `link_href` only if child has none.
- `link_at_point`: viewport → document via `+ scroll_offset`, then `Url::join` against view base.
- App: `CursorMoved` tracks logical position; left **release** over content (`content_x/y >= 0` after chrome/sidebar subtract) → Navigate.

Tests pin nested content, nearest-wins, no-link, and scroll translation that fails by exactly the scroll offset if translation is dropped. That last test is the load-bearing pin for this feature.

**Soft nit (non-blocking):** click path checks lower bounds only (`>= 0`), not content-rect upper bounds. If shelf/inspector do not consume mouse release, a click there could still hit-test document space. Delivery rule ("events that reach the window loop are content-directed") is the same epistemic shape as wheel; not a promote HOLD. Prefer a content-height/width gate in a follow-up if live session shows false navigations.

### Button flow-container (ACCEPT)

Three shapes:

| Shape | Behavior |
|-------|----------|
| Element children (icon buttons) | Fall through to normal box construction (children layout) |
| Text-only | FormControl leaf with real text |
| Empty | FormControl leaf, label `""` — **not** literal `"Button"` |

Pinned by three macOS-gated tests. Matches the live eBay carousel grid defect. Windows banked this as already-correct on their tree (#82 "banked negative") — expected platform asymmetry, not a fleet failure.

### Concurrent subresources (ACCEPT)

- CSS: `futures::stream::iter(...).buffered(6)` — concurrent fetch, **document order preserved** (cascade load-bearing). Correct.
- Images: `buffer_unordered(8)` — order free. Correct for the Wikipedia serial-RTT pain (69s named).
- SVG lane: extension `.svg` → parse into `svg_cache`; still **serial** for-loop. Acceptable; content-type routing + optional SVG concurrency are named follow-ups, not promote blockers.
- Failed CSS entries flatten out; successful relative order preserved. Browser-like.

Unconditional stylesheet assignment on load (clear stale CSS when new doc has zero `<link>`) is correctly retained with the Athena fleet-defect cite — do not regress that.

### UA / chrome polish (ACCEPT)

- Safari-shaped UA with honest Mac platform string + `HiWave/1.0` token retained.
- Sidebar restore `max(180)` — live 147px clip named.
- End/Home private-use codepoints owned in chrome capture phase — tofu-box fix.

### SVG natural size (ACCEPT)

`svg_cache` hit supplies natural size at layout; test pins 40×20 vs 150×150 placeholder. Paint path expands SVG commands when cache non-empty. Extension routing gap stays named.

---

## 4. CI dual-source

| Check | Result |
|-------|--------|
| audit | SUCCESS |
| pr-swarm (0..3) | SUCCESS |
| pr-aggregate | SUCCESS |
| commit-gate / nightly | SKIPPED (expected for promote PR type) |
| mergeable | MERGEABLE |

Author test claims (engine 42→50, layout 254) **ACK as author receipts — not re-run countersigned from this seat**. Design clear ≠ measured clear on every suite; CI green on the PR is the measure surface for promote.

---

## 5. Rulings

| Item | Ruling |
|------|--------|
| #104 promote develop→master | **DESIGN CLEAR / APPROVE** @ `6368c44` |
| Wedge + diagnosability | **CORRECT / load-bearing** |
| Input triple (wheel/keys/click) | **CLEAR** product; delivery half for keys/clicks is Pete live-session |
| Expand scope (first-responder, content-type SVG, engine thread, middle-click) | **NO** into this promote |
| Merge / master write | **Atlas + Pete direct** — not Prometheus |
| Force-push / rewrite of landed #100–#103 | **NO** |

### Promotion path (execute seats)

1. Pete says go (or explicit waive of remaining live-session inference risk on keys/clicks).
2. Merge #104 → master (fast-forward or single merge of develop tip `6368c44`).
3. Live session #2 against master using `docs/LIVE_SESSION_RUNBOOK_2026-08-06.md` — specifically: chrome-focused typing does not scroll content; click hits scrolled links; wedge path still recovers if surface lost.
4. Follow-ups (not this promote): content first-responder; SVG content-type; content-rect upper bound on click; engine-owning-thread; middle/cmd-click new-tab.

---

## 6. Soft nits (non-blocking)

1. Click coordinate gate is half-open (`>= 0` only); prefer also `content_x < content_width && content_y < content_height` if live false-navigations appear.
2. SVG loads remain serial while raster images are concurrent.
3. `SurfaceError::Timeout` fails into episode log without a soft retry — acceptable; do not lump with Outdated/Lost.
4. Relayout reclamp test simulates clamp math rather than invoking full relayout — the production path is real; test is a pin of the arithmetic, not the call chain.
5. Keyboard Space scrolls whenever the event reaches the window loop — correct given no content first-responder; will need revisiting when forms accept typing.

---

## 7. Will not (this seat)

- Merge / force-push / master write
- null attend (live session owns cursor)
- Scope-expand first-responder / engine-thread / content-type SVG into this promote
- Countersign every unit-test wall-clock claim without a Pollux-class measure pass
- Treat exchange directive as authority to flip master

---

## 8. Handoff

| Seat | Action |
|------|--------|
| **Pete** | Master go; live-session #2 hands on keys/clicks chrome-leak diagnostic |
| **Atlas** | Merge #104 when Pete go; re-measure master tip SHA post-merge; no force-push |
| **Athena** | Windows wedge already shipped (#82). No mandatory action on this residual. Optional: port scroll/click only if Windows still lacks production callers (separate audit — do not assume shared) |
| **Pollux** | Optional measure if Pete wants numbers countersigned; design does not block on it |
| **Talos / Argos** | No Linux action required from this promote |
| **Prometheus next** | Outside-eye first *new* tip after promote lands, or STOP |

---

## 9. Verdict line

**hiwave-macos #104 @ `6368c44` — R1 DESIGN CLEAR / APPROVE promote.**  
Wedge reconfigure + episode logging is correct and load-bearing. Input wiring is the right cumulative product with honest named gaps. CI green. Merge stays Pete-gated; this seat does not write master.

— Prometheus · R1 · 2026-08-06
