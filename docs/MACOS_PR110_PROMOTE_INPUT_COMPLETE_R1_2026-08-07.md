# Outside-eye R1: hiwave-macos PR #110 (promote develop→master — content input complete)

**Date:** 2026-08-07  
**Seat:** Prometheus (design only)  
**Tip measured:** `a60ecac` (≡ `origin/develop`)  
**Master measured:** `da8f0fc` (#112 WPT honesty MERGED)  
**PR:** https://github.com/hiwavebrowser/hiwave-macos/pull/110  
**Base:** master · **MERGEABLE** · CI audit+swarm×4+aggregate **SUCCESS** (Actions dispatching again)

## Verdict

| Item | Ruling |
|------|--------|
| #110 cumulative product | **DESIGN CLEAR / APPROVE promote** @ `a60ecac` |
| Engine-side edit state (not DOM mutability) | **CLEAR** — correct IDL-value side table |
| Edit-state lifecycle (#111) | **CLEAR** — per-view; clear on `load_url`/`load_html` |
| Live viewhost deadlock (#116) | **CLEAR** — copy-drop-call on live `lib.rs` path |
| Clicks via view-local queue (#115) | **CLEAR** — window-loop path proven dead |
| Form submit GET-only / POST None | **CLEAR** (honest decline) |
| Preflight-promote script | **CLEAR** |
| Merge-tree develop→master | **CLEAN** · #112 WPT honesty **preserved** |
| Product banner "typing verified e2e" | **HARD NO** until live key-delivery receipt |
| Keys still on window loop | **SOFT residual** — measure before claiming writable |
| Orphan twin `MacOSViewHost` in macos.rs | **SOFT** — documented; zero callers |
| Merge | **Atlas + Pete** — not Prometheus |

## Board (re-measured this tick)

| Surface | Tip / state |
|---------|-------------|
| macOS **#110** | tip **`a60ecac`** · OPEN · MERGEABLE · **NEW residual past prior f3d878f ask** · CI SUCCESS |
| macOS master | **`da8f0fc`** (#112 MERGED 2026-08-06T23:37Z) |
| macOS develop | **`a60ecac`** · ahead 23 / behind 4 vs master (**diverged**) |
| macOS **#112** | **MERGED** · prior CLEAR banked |
| macOS **#99** | **CLOSED** · SUPERSEDE stands |
| Win | open **#33 HOLD only** · develop `67ec265` · master `f0c2f5a` |
| Linux **#59** / **#58** | OPEN · CLEAR banked @ `b662494` / `387a8ee` |
| umbrella #11 | OPEN · HARD AMEND banked · tip `d141f26` |
| community / tank | open **zero** |
| CI-void | Actions **dispatching** on macos again; re-arm of green-before-merge still **Pete's word** (substitute receipts continue until then). develop→master R1 **unchanged**. |

## Scope (product vs docs)

| Class | Paths | Δ (product-ish) |
|-------|-------|-----------------|
| Product crates | `hiwave-app` main+webview · `rustkit-engine` · `rustkit-layout` · `rustkit-viewhost` lib+macos | **+1645 / −98** across 8 product files (+ preflight) |
| Ops | `scripts/preflight-promote.sh` | new |
| Docs/trench | R1 bank + runbook + baseline digests | non-product |

PR body still says "5 PRs / tip f3d878f". **Live tip includes more** (must re-measure cumulative):

| PR / SHA | What |
|----------|------|
| #105 | #104 soft nits (click gate bounds, SVG concurrency) |
| #106 | DOM `node_id` on layout boxes — unblocks hit→focus |
| #107 | Form typing via orphaned `TextEditState` |
| #108 | Lock-across-AppKit on **orphan twin** (macos.rs) |
| #109 | Enter submits form (GET only) |
| #111 | Edit state per-view + dies with document (**prior must-fix**) |
| #113 | Relative image URL resolve against document base |
| #114 | Click/wheel diag instrumentation |
| #115 | Content clicks via `RustKitContentView` queue |
| #116 | Deadlock class on **LIVE** `lib.rs` focus/set_bounds/set_visible |

## Independent ground (not PR prose)

Worktrees: `/tmp/hiwave-pr110-r1` @ `a60ecac` · `/tmp/hiwave-pr110-master` @ `da8f0fc`.

### 1. Parent defects CONFIRM

| Defect | Master (`da8f0fc`) | Tip |
|--------|-------------------|-----|
| Form typing | no `edit_states` / no `handle_text_key` | engine-side map + keyboard model wired |
| Focus hit-test | node_id TODO still in engine comments | `LayoutBox.node_id` stamped on real build path |
| Clicks | window-loop `MouseInput` only | view-local queue + drain (window path still present as diag dead end) |
| Focus lock | `lib.rs` holds state lock across `makeFirstResponder`/`SetFocus` | copy hwnd → drop guards → platform call |
| Relative images | raw attr vs absolute cache key mismatch | `resolve_resource_url` at build |

### 2. Engine-side edit state — DESIGN RULING

Atlas asked: engine `HashMap<node_id, TextEditState>` + layout read-through vs DOM mutability.

**APPROVE engine side table.** Reasons measured in tree:

- Doc comment correctly frames IDL-value vs content attribute; form-reset stays possible later.
- `NodeId` is per-document (counter restarts at 1) — **global** map would collide across views/docs.
- #111 moved map onto the **view** and clears on both `load_url` (~L1198) and `load_html` (~L1344).
- Load-bearing test `typed_text_does_not_survive_a_navigation_into_the_next_page` exercises **real** `load_html` path and asserts id collision precondition + cleared map.
- Seed-once on first focus (`!already_seeded`) preserves user typing across re-focus.

**Do not** grow DOM mutability for this residual. Side table + explicit lifetime is the cheaper correct shape.

### 3. Deadlock class — LIVE vs twin

| Path | Status |
|------|--------|
| `ViewHost::focus/set_bounds/set_visible` in `lib.rs` | **LIVE** · #116 copy-drop-call · macOS `makeFirstResponder` after drop |
| `MacOSViewHost::*` in `macos.rs` | **ORPHAN twin** · same fix applied in #108 · **zero callers** of `create_view_from_window` |
| Live create | `lib.rs` `create_view` uses `rustkit_content_view_class()` · comment names the twin trap |

Callers of create: `webview_rustkit.rs` / chrome → trait/`ViewHost::create_view` only. Twin stack is the eleventh orphan instance this week — documented, not a merge blocker. **SOFT residual:** delete or `#[cfg(test)]` the twin later so the next seat cannot re-patch the wrong function.

### 4. Clicks CLEAR; keys SOFT

Measured shape (Atlas + tree):

- Child NSView receives clicks; they **do not** surface as tao window `MouseInput`.
- Fix: `RustKitContentView` records into `PENDING_CLICKS`; app drains per loop turn; coords already viewport (chrome math deleted).
- Queue-not-callback: avoids #108 re-entrancy inside AppKit dispatch.

Keys still use `WindowEvent::KeyboardInput` with `has_focused_element()` gate. After focus, `makeFirstResponder` is on the content view — key delivery may still bubble (no `keyDown:` override) **or** may not. Atlas named this **UNKNOWN**.  

**Ruling:** architecture for typing is CLEAR; **product claim "content is writable" is not countersigned** until a synthetic or live key-delivery receipt. Do not treat green unit tests as e2e typing. Named residual, not a redesign.

### 5. Form submit

- `form_submission_for_focus`: walk to `<form>`, GET only, POST → `None` with debug (not silent method flip) — **CLEAR**.
- Values from live edit state when present.
- Tests cover typed value, unsuccessful controls, POST decline, no-form field.

### 6. Relative image URLs (#113)

Root cause confirmed in tip comments: load caches absolute; layout/paint used raw attr. One resolve point at build. **CLEAR** for this promote (no expand to content-type SVG routing).

### 7. Merge-tree / #112 coexistence

| Direction | Result |
|-----------|--------|
| develop → master (promote) | automatic merge **CLEAN** · product files from develop · **wpt_tier1.py stays master 459-line honesty** (md5 match) |
| master → develop | automatic merge **CLEAN** · brings #112 onto develop |

**HARD residual lifted** for "will promote eat #112?" — **NO** if standard merge commit.  

**SOFT process:** merge master into develop first so develop becomes content-complete ancestor of post-promote master (branching-model hygiene; preflight-promote only guards delete_branch_on_merge).

### 8. Preflight-promote

Refuses open when `delete_branch_on_merge=true`; `--post-merge` verifies develop survived without auto-heal. macOS setting already false. **CLEAR**.

### 9. CI

PR checks SUCCESS after Actions returned. CI-void re-arm still Pete-gated; this R1 does not re-arm fleet policy.

## What is NOT approved / named residuals

1. **No human / synthetic key e2e** for typing or Enter-submit on the live app path.
2. **Keys may share the click dead-end class** until measured — next unit if red: view-local key queue (same shape as clicks).
3. **POST forms** need `load_url` body variant — separate unit.
4. **Twin stack** still in tree — docs/delete later.
5. **Windows/Linux input** — do not port window-loop click shape (Atlas pin); Argos Linux twin hunt = hollow shared API, different class.
6. **#33 HOLD / #59 CLEAR / #58 CLEAR / #11 HARD AMEND** — unchanged.

## Seat tasking

### Atlas
1. Land #110 when Pete go (merge commit; no force-push).
2. Optional: merge master→develop first for ancestor hygiene.
3. After promote: re-measure master tip; rebuild demo binary from master.
4. First live session probe: synthetic keyDown through content view + typing into a search field.
5. Do not banner "writable browser" without that receipt.

### Pete
1. Master go for #110 (promote still Pete-gated).
2. Explicit CI green-before-merge re-arm when ready (Actions are dispatching on macos; do not leave exemption ambient).

### Athena / Talos
1. Input delivery: measure platform path the way Atlas measured AppKit — do not assume window-loop clicks.
2. Deadlock class: audit live focus/set_bounds paths if not already (#85 Windows banked).

### Argos
Optional greps on tip: `edit_states.clear` on both load paths; `drain_pending_clicks`; live `lib.rs` focus drops guards before `makeFirstResponder`; no `crates/` loss of #112 (wpt only on master side).

## No irreversible from this seat

No merge, force-push, spend, master write, or null attend.

— Prometheus
