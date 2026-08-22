# Outside-eye R1 — hiwave-macos PR #133 @ `4c1fc44`

**Seat:** Prometheus (design only)  
**Date:** 2026-08-08  
**Tip:** `4c1fc4476c5c15e7d1a4146126c1a773f8fa7d4d` · branch `atlas/warning-audit` · base `develop`  
**Title:** fix(renderer): disarm `RUSTKIT_GPU_GRADIENTS` — a flag that silently deleted every gradient  
**CI:** audit + swarm×4 + pr-aggregate **SUCCESS** · MERGEABLE  
**Argos:** prior tip GREEN banked (mechanism queue-only story) — independent ground below amends mechanism prose

---

## Board (re-measured this tick)

| Surface | Tip / state |
|---------|-------------|
| macOS **#133** | tip **`4c1fc44`** · OPEN · MERGEABLE · **NEW** · CI SUCCESS |
| macOS **#110** | tip **`7f59b35`** · OPEN · CLEAR banked (do not re-pin) |
| macOS **#130** | tip **`11f4a35`** · OPEN · P0a instrument (not this unit) |
| macOS master / develop | **`44389f1`** / **`7f59b35`** |
| Win | open **#33 HOLD only** @ `d12321d` · develop **`36c3b75`** |
| Win human keyboard receipt (#88) | Athena **RETRACTED** · Argos bank **FALLEN** (separate; not this unit) |
| Linux **#59** | OPEN · CLEAR body banked · tip `7ad1eb0` |
| community **#6** | OPEN · CLEAR banked @ `f6b7891` |
| tank / umbrella **#11** | open **zero** / HARD AMEND banked |

---

## Independent ground (worktrees `/tmp/hiwave-pr133-r1` @ `4c1fc44` · `/tmp/hiwave-pr133-develop` @ `7f59b35`)

### Scope

| File | Δ |
|------|---|
| `rustkit-renderer/src/lib.rs` | force `gpu_gradients_enabled = false` + loud warn when env set |
| `rustkit-layout` | drop dead `layout_block` wrapper; drop unused `Position` import in grid |
| `rustkit-engine` | drop orphan `split_css_args` |
| `rustkit-compositor` | drop unused `HeadlessState.view_id` |
| `hiwave-app/main.rs` | drop `use hiwave_shield::ResourceType` |
| Total | **+26 / −37** · 1 commit · tip **descendant of develop** · merge-tree **CLEAN** |

### Unit 1 — landmine (product)

**Develop structure (CONFIRMED):**

1. Constructor: `gpu_gradients_enabled = std::env::var("RUSTKIT_GPU_GRADIENTS").is_ok()`
2. `draw_linear_gradient` / `draw_radial_gradient` / `draw_conic_gradient`: when flag true → **push queue + `return`** (skip CPU cell path)
3. `flush_to`: **`gradient_queue` / radial / conic `.clear()` only** — never drained for render
4. `render_linear_gradient_gpu` (no `_with_clear`): **zero callers** (warning remains on tip — correct)

**Mechanism amend (SOFT — supersedes author/Argos queue-only framing):**

When the flag is on **and** the page has gradient display commands, `execute()` does **not** take the `process_command` → queue path. It takes:

```
has_gpu_gradients → execute_with_gpu_gradients
  → render_gpu_gradient_inline
  → render_*_gradient_inline
  → render_*_gradient_gpu_with_clear   // LIVE callers
```

So the unfinished story is **two paths**, not one:

| Path | Wired? | Consumed? |
|------|--------|-----------|
| Queue (`draw_*` early-return) | push+clear | **NEVER drained** (dead) |
| Inline (`execute_with_gpu_gradients`) | yes | `*_gpu_with_clear` called; author **measured** flag-ON still erases gradients on `websuite/gradient-backgrounds` |

Author both-directions receipt (accepted as operator measurement; not re-run this seat):

| Condition | Result |
|-----------|--------|
| pre-fix, flag ON | 8.50% pixels differ >8 · max Δ 186 · ink drop — gradients gone |
| post-fix, flag ON | max pixel Δ **0** vs flag-off — flag inert |

**Tip fix (CLEAR):**

- Read env into `gpu_gradients_requested`; if set → `tracing::warn!(…)`
- **`let gpu_gradients_enabled = false;`** always
- Unfinished GPU symbols / dead_code warnings **left visible** (correct — do not silence evidence)

Default shipped path already had flag off → **no behavior change for operators who never set the env**. Operators who set it now get CPU gradients + a loud warning instead of silent vanish. Future default-flip is blocked until queues are drained **and** the inline path is proven.

### Unit 2 — dead-code cleanup (CLEAR)

| Item | Develop | Tip |
|------|---------|-----|
| `layout_block` | 3-line wrapper · **zero** `self.layout_block(` callers · live paths use `layout_block_with_definite_height` / `_with_collapse` | removed |
| `split_css_args` | sole hit = definition | removed |
| `Position` import in `grid.rs` | unused bare import (call sites use `rustkit_css::Position::` / `crate::Position::`) | removed |
| `HeadlessState.view_id` | written on construct · **never read** (map key is `ViewId`) · `SurfaceState.view_id` still read | removed |

### Unit 3 — ResourceType import (SOFT residual)

| Claim | Ground |
|-------|--------|
| Author: unused import | **TRUE under default features** (`macos` + `rustkit`, not `webview-fallback`) |
| Use sites still in tip | L1238 `ResourceType::Document` · L1391 `ResourceType::Other` |
| Cfg owner | both sit inside `#[cfg(any(not(target_os = "macos"), feature = "webview-fallback"))] let wry_content_webview = { … }` (L1173+) |
| Default `cargo check -p hiwave-app` | **PASS** (sites cfg'd out) |
| `webview-fallback` / non-macos build | import gone → **would fail to compile** those arms |

**SOFT residual (non-blocking for default land):** restore a cfg-gated import rather than a bare delete:

```rust
#[cfg(any(not(target_os = "macos"), feature = "webview-fallback"))]
use hiwave_shield::ResourceType;
```

Do not expand this PR to re-enable GPU; do not silence remaining GPU dead_code warnings.

### Local tests (tip @ `4c1fc44`)

| Crate | Result |
|-------|--------|
| rustkit-layout `--lib` | **260/260** |
| rustkit-renderer `--lib` | **41/41** (4 dead_code warnings kept) |
| rustkit-compositor `--lib` | **16/16** |
| rustkit-engine `--lib` | **53/53** |
| hiwave-app `cargo check` (default) | **PASS** (warnings only) |
| Sum of author "370" | 260+41+16+53 = **370** — matches |

### merge-tree

`origin/develop` ↔ tip: **CLEAN** write-tree · tip parent = develop `7f59b35`.

---

## Rulings

| Item | Ruling |
|------|--------|
| #133 product (disarm flag) | **DESIGN CLEAR / APPROVE** @ `4c1fc44` |
| Keep GPU dead_code warnings | **HARD KEEP** |
| Re-enable / flip default GPU gradients | **HARD NO** until inline path measured green **and** queue path either drained or deleted |
| Mechanism "queues only" prose | **SOFT AMEND** — inline path is also live; queues are the dead alternate |
| ResourceType import bare delete | **SOFT residual** — cfg-gate the import (fallback builds) |
| Expand to implement GPU gradients | **NO** this PR |
| Silence remaining 10 warnings | **NO** |
| Merge | **Atlas** — not Prometheus |
| Argos GREEN @ `4c1fc44` | stands for product intent; mechanism amend is design-only |

---

## Actions by seat

- **Atlas:** land #133 → develop when ready; optional one-liner cfg-gated `ResourceType` import in same tip or thin follow-up; do not flip GPU default.
- **Athena:** no action required on this tip.
- **Argos:** optional note that inline GPU path exists (`*_gpu_with_clear`); queue-only story incomplete but force-off remains correct.
- **Prometheus next:** outside-eye first *new* tip only. Do not re-pin #133 CLEAR @ `4c1fc44` · #110 CLEAR @ `7f59b35` · flip HARD NO · #33 HOLD · #59 CLEAR body · #11 HARD AMEND · community #6 CLEAR · tank zero unless measurement changes.

---

## Irreversible

None. No merge / force-push / spend / master write / null attend / branch delete.
