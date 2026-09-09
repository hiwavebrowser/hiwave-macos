# Outside-eye R1 residual: hiwave-macos PR #110 tip `7f0f84d`

**Date:** 2026-08-08  
**Seat:** Prometheus (design only)  
**Tip measured:** `7f0f84d` (≡ `origin/develop`)  
**Prior banked CLEAR:** `a60ecac` (2026-08-07) — **ancestor; tip MOVED**  
**Master measured:** `44389f1` (#119 nullable-diff-guard MERGED; #118 ruby on master)  
**PR:** https://github.com/hiwavebrowser/hiwave-macos/pull/110  
**Base:** master · **MERGEABLE** · CI audit+swarm×4+pr-aggregate **SUCCESS** (2026-08-08T00:35–00:47Z)

## Verdict

| Item | Ruling |
|------|--------|
| #110 promote @ tip residual | **DESIGN CLEAR / APPROVE** @ `7f0f84d` |
| Prior CLEAR @ `a60ecac` | **STANDS** for that SHA only; do not re-pin body |
| Keys follow focus (#120) | **CLEAR** design path (AppKit first-responder + drain + edit model) |
| Product banner "typing verified e2e" | **HARD NO** until Pete live-session receipt (Atlas: fingers, not more code) |
| Nav Back/Forward/Reload (#122) | **CLEAR** — RustKit `nav_*`, not `evaluate_script` stub |
| WebP decode (#123) | **CLEAR** — detect→decode; engine path proven |
| object-fit initial fill (#125) | **CLEAR** product path (ComputedStyle + layout arm) |
| ObjectFit enum `#[default]` still Contain | **SOFT residual** — non-blocking; unknown `from_css` still Contain |
| @font-face Q1 parse (#124) | **CLEAR** parse-only quarter held |
| font load / engine wire | **HARD residual** — `load_font` hollow; `font_face_rules()` **zero** product callers |
| data-URI `;` tokenizer fix | **CLEAR** (shared surface; load-bearing) |
| srcset (#126) | **CLEAR** subset (widest `w` / densest `x`; no `sizes`) |
| merge-tree develop→master | **CLEAN** (`git merge-tree --write-tree` OK) |
| Merge | **Atlas + Pete** — not Prometheus |

## Board (re-measured this tick)

| Surface | Tip / state |
|---------|-------------|
| macOS **#110** | tip **`7f0f84d`** · OPEN · MERGEABLE · **NEW residual past a60ecac CLEAR** · CI SUCCESS |
| macOS master / develop | **`44389f1`** / **`7f0f84d`** |
| macOS open PRs | **#110 only** (feature branches for #120–#126 deleted = merged to develop) |
| Win | open **#33 HOLD only** @ `d12321d` · develop moved `b79c008` |
| community **#6** | OPEN @ `f6b7891` · Prometheus R1 + Argos GREEN **banked** |
| tank / umbrella | open **zero** |
| Campaign lanes | WebP+object-fit+srcset+font Q1 **landed develop**; abs-pos / inline SVG / Q2 wire **not yet PR tips** |

## What moved past banked CLEAR `a60ecac`

| Merge / SHA | Unit | Class |
|-------------|------|-------|
| #120 `528a123` | keys follow focus + `type=hidden` no box | product |
| #121 `424a4ae` | rendering-gap plan (ratified) | docs |
| #122 `fc32763` | Back/Forward/Reload → real nav | product |
| #123 `86250b8` | WebP decode (`image-webp`) | product |
| #125 `8761ab6` | object-fit initial = fill | product |
| #124 `45cc7e3` | @font-face parse + data-URI `;` fix | product |
| #126 `cb22d99` | srcset pick (subset) | product |

Δ tip-only vs prior CLEAR: **+1333 / −76** across 19 paths (engines + codecs + plan doc). Full promote vs master still ~31 files / +4248.

## Independent ground (worktrees)

`/tmp/hiwave-pr110-tip` @ `7f0f84d` · `/tmp/hiwave-pr110-master-tip` @ `44389f1`.

### 1. Keys (#120) — parent defect CONFIRMED; wire CLEAR

| Check | Result |
|-------|--------|
| Master KeyboardInput | scroll-only (arrows/page/space) · **no** text path |
| Tip viewhost | `acceptsFirstResponder` + `keyDown:` → `PENDING_KEYS` · **CONFIRMED** |
| Tip drain | main loop `drain_pending_keys` → VK map → `handle_text_key` · **CONFIRMED** |
| Window path | dual: when `has_focused_element()`, tao KeyboardInput also edits |
| `grab_keyboard` | first real `focus_view` caller (deadlock class #116 load-bearing) |
| `type=hidden` | no layout box · HTML §4.10.5.1.1 · **CONFIRMED** |
| Live e2e receipt | **UNMEASURED this seat** — Atlas holds for Pete fingers |

**Ruling:** design path is no longer the SOFT residual from `a60ecac` R1. Product claim "typing works in a live session" stays **HARD NO** until receipt.

### 2. Nav buttons (#122) — CLEAR

Master/old stub: `evaluate_script("history.back()")` is a silent no-op on RustKit (no page JS). Tip: `v.nav_back()` / `nav_forward()` / `nav_reload()` under rustkit feature. **CLEAR.**

### 3. WebP (#123) — parent hollow CONFIRMED; tip CLEAR

| Master | Tip |
|--------|-----|
| detect WebP → `Unsupported` | `decode_webp` via `image_webp` → RGBA8 |
| | engine fixture test `webp_reaches_the_engine` |
| | animated → frame 1 (stated) |

### 4. object-fit (#125) — CLEAR with SOFT enum nit

| Site | Master | Tip |
|------|--------|-----|
| `ComputedStyle::new().object_fit` | `"contain"` | **`"fill"`** + unit test §5.5 |
| layout paint match `_` | (prior) | **`ObjectFit::Fill`** |
| `ObjectFit` enum `#[default]` | Contain | **still Contain** |
| `from_css` unknown | → default = Contain | unchanged |

Product path for unspecified property is the ComputedStyle string → layout arm. Enum Default is only hit by `from_css` garbage / direct Default. **SOFT** follow-up: flip `#[default]` to Fill for consistency. Not a merge block.

### 5. @font-face Q1 (#124) — CLEAR scope held

| Check | Result |
|-------|--------|
| `rustkit-css/src/font_face.rs` | **NEW** · parse + `Stylesheet::font_face_rules()` |
| Master | file absent |
| cssparser data-URI | paren/quote depth · `;` inside `url()` no longer splits · **CONFIRMED** |
| engine callers of `font_face_rules` | **ZERO** outside css tests |
| `load_font` | still `data: Vec::new()` hollow · **CONFIRMED** |
| Quarter claim | parse only · Q2 wire is next · **honest** |

Do **not** banner fonts-on-page. Do **not** expand this residual into loader work.

### 6. srcset (#126) — CLEAR subset

`pick_from_srcset`: partition width vs density; widest `w` else densest `x`; bare = 1x. Explicit non-claim: no `sizes`, no viewport/DPR math. Tests pin only-srcset → layout URL. **CLEAR.**

### 7. Merge hygiene

| Check | Result |
|-------|--------|
| `a60ecac` ancestor of tip | YES |
| tip ≡ develop | YES `7f0f84d` |
| merge-tree write-tree vs master | **CLEAN** tree `f66ce6a…` |
| CI | audit + swarm 0–3 + aggregate SUCCESS |

## Hard / soft bank

| Rule | Status |
|------|--------|
| No e2e typing banner without Pete receipt | **HARD** |
| No "webfonts work" until Q2+ fetch register | **HARD** |
| No Phase 2 engine extract this promote | **HOLD** (campaign pin) |
| No livesuite "near-complete" banner | **HARD** (plan soft pin P2) |
| ObjectFit enum Default → Fill | **SOFT** optional follow-up |
| srcset + sizes | separate unit |
| Win/Linux engine freeze | stands (reference = macOS) |

## Seat handoffs

| Seat | Action |
|------|--------|
| **Atlas + Pete** | Land #110 when Pete go; optional master→develop hygiene if still diverged ancestry |
| **Pete** | Key-delivery live receipt (fingers); master promote word |
| **Athena** | Q2: wire `font_face_rules()` → loader (do not claim load until fetch+register) |
| **Talos** | Abs-pos containing-block first tip (S2); then inline SVG |
| **Atlas** | Remaining image sizing if any; livesuite freezer track |
| **Argos** | Tip re-R1 optional on promote; F1–F4 on campaign merges |
| **Prometheus** | Next: first *new* open tip from S2/S3 residual or post-promote land. **Do not re-pin this CLEAR** |

## Not done by this seat

- No merge / force-push / master write  
- No null attend  
- No product code  
- No Phase 2 extract  
- No re-pin of banked CLEARs (#118/#117/#112/#59/#58/#33 HOLD/#11 HARD AMEND/community #6)

— Prometheus · grind tick 2026-08-08 · one unit · stop
