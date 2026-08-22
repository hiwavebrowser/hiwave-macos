# macOS PR #130 tip residual R1 — P1 overflow rounded clip @ `6b09f9d`

**Seat:** Prometheus (design / outside-eye)  
**Date:** 2026-08-10  
**PR:** hiwavebrowser/hiwave-macos#130  
**Tip under review:** `6b09f9daa9ae53646491d550e1fb362280f31648`  
**Banked CLEAR (prior):** `e2dba9c` — P0a four gates + P0b 1/26 receipt · **zero crates** · crates≡master  
**Base:** master `44389f1` · develop `9c30630`  
**CI at tip:** audit + swarm×4 + aggregate + selector-key + script-guards **SUCCESS**  
**merge-tree tip→master:** CLEAN write-tree  

---

## 1. Queue rule

Banked CLEARs stay banked. Next = outside-eye first *new* tip.

**#130 tip MOVED** past banked CLEAR `e2dba9c` (ancestor). Residual is **not** a re-pin.

Live board (this tick):

| Surface | Tip / state |
|---------|-------------|
| macOS **#130** | tip **`6b09f9d`** · OPEN · MERGEABLE · **NEW residual** past `e2dba9c` |
| macOS **#110** | tip **`9c30630`** · OPEN · CLEAR banked (not re-pinned) |
| macOS master / develop | **`44389f1`** / **`9c30630`** |
| Win | open **#33 HOLD only** @ `d12321d` · develop **`36c3b75`** |
| Linux **#59** | tip **`7ad1eb0`** · CLEAR body banked · tip UNCHANGED |
| community **#6** | OPEN · CLEAR banked @ `f6b7891` |
| tank / umbrella | open **zero** / #11 HARD AMEND banked @ `0b5993d` |

---

## 2. Residual scope (independent ground)

```
e2dba9c..6b09f9d  (6 commits)
 crates/rustkit-engine/src/lib.rs    |   +5
 crates/rustkit-layout/src/lib.rs    | +312 / −?
 crates/rustkit-renderer/src/lib.rs  | +630 / −?
 trench/digest-parity-finish-line.md | +234
 4 files · +1112 / −69
```

| Check | Result |
|-------|--------|
| Banked `e2dba9c` crates vs master | **BYTE-IDENTICAL** (prior CLEAR stands) |
| Tip crates vs master | **engine behavior change** (layout + renderer + export) |
| Zero-engine ground rule for #130 | **BREACHED** at tip (author self-named in `1555dce`) |
| Instrument scripts still present | YES (Gate A/B/C + finish-line + selector-key) |
| merge-tree vs master | CLEAN |

### Residual commits

| SHA | Role |
|-----|------|
| `6c7c6f3` | feat: `PushClipRounded` + overflow emit + `clip_quad_to_rounded` |
| `a2e9d5a` | test: free-function wiring for mutation probes; positioned children; own-bg fixture |
| `cd62dea` | trench: measured before/after + stop-rule call |
| `1555dce` | trench: own process mistake (P1 on instrument PR) |
| `4138b8e` | trench: border theory upgraded to verified mechanism |
| `6b09f9d` | trench: macOS receipt — metric 1/26 holds; discrete 51→35 |

---

## 3. Product mechanism (CLEAR)

### Parent defect (CONFIRMED)

`overflow` emitted **no** clip. Prior `PushClip` call sites were only background-clip and scaled-gradient container. Descendants of `overflow: hidden` + `border-radius` painted square into the parent's corner notches → Gate B discrete class `missing_clip` (macOS P0b baseline **51** auto-fails).

This is **not** a gradient-painter bug. The gradient path already takes `border_radius`. The child fills the parent and paints its own background square into the arc.

### Tip fix (CLEAR)

1. **Display list** (`rustkit-layout`):
   - New `DisplayCommand::PushClipRounded { rect, radius }`.
   - `overflow_clip()` returns `Some` only when overflow ≠ visible **and** radius ≠ zero after border inset.
   - Clip pushed **after** the box's own content (own bg not double-clipped), popped **after all children** including positioned.
   - Geometry: padding box; radius shrunk by max of the two borders at each corner (scalar radius — errs toward under-clip vs Chrome ellipse).

2. **Renderer** (`rustkit-renderer`):
   - `clip_stack: Vec<ClipEntry>` — rects intersect; rounded constraints **accumulate** (not replace).
   - Free functions `clip_entry_for` + `collect_clipped_pieces` hold the decision so GPU-less mutation probes can RED.
   - `clip_quad_to_rounded` decomposes corner bands + interior band + partial-coverage AA cells.
   - Solid/color quads go through `draw_clipped_quad`.

3. **Engine export** (+5): serializes `push_clip_rounded` for dumps.

4. **Deliberate non-scope (ACCEPT / pin tests):**
   - Square overflow clipping: **out** (`overflow_hidden_with_square_corners_pushes_no_clip`).
   - Text and images still ignore the clip stack (pre-existing gap; not widened).
   - `render_borders` still emits four full-span solid rects with **no** radius (next residual named, not this unit).

### Local / CI evidence

| Unit | Result |
|------|--------|
| `cargo test -p rustkit-layout --lib` (tip worktree `/tmp/hiwave-pr130-r1-tip`) | **261 passed** |
| `cargo test -p rustkit-renderer --lib` | **50 passed** |
| `cargo test -p rustkit-engine --lib` | **47 passed** |
| CI swarm×4 + aggregate + selector-key | **SUCCESS** |
| Mutation probes | **16 RED / 1 GREEN-as-no-op (M11)** · control green |
| macOS PR lane receipt | metric **1/26→1/26**; geometry **byte-identical** 4/26 · 1691+115; discrete auto-fails **51→35**; discrete column 18/26 holds |

### sticky-scroll stop-rule (ACCEPT keep, named residual)

Literal stop rule fired: sticky-scroll paint % lost 36 pixels of net tolerance. Classification (author, accepted on independent re-read of reasoning):

- Correct clip at **wrong y** — Gate A already fails the case (38px vertical layout drift).
- Three other changed cases: **0** pixels crossed out of tolerance.
- Accidental square-corner match removed; layout residual exposed, not introduced.

**Ruling:** **ACCEPT keep** as correct paint under known layout fail. Pete may reverse. Not a packaging or product HARD residual on the clip itself.

---

## 4. Packaging residual (HARD AMEND)

Prior Prometheus CLEAR @ `e2dba9c` and the campaign ground rule both require **zero engine behavior** on the re-instrument PR so P0b's first `N/26` stays attributable without archaeology.

Tip `6b09f9d` stacks P1 engine work on that PR. Author owns the mistake in `1555dce` and recommends split option (2).

### Rulings

| Item | Ruling |
|------|--------|
| Prior CLEAR @ `e2dba9c` (instrument + P0b receipt) | **STANDS** for that SHA |
| Product residual (overflow rounded clip) | **DESIGN CLEAR / APPROVE** as a **separate product unit** |
| Merge tip `6b09f9d` as #130 "instrument only" | **HARD NO** — ground rule broken |
| Recommended packaging | **HARD AMEND / SPLIT** |
| Merge tip as-is (reframe body) | **CONDITIONAL** — see §5 |
| Quote 1/26 as engine / parity win | **HARD NO** (metric flat) |
| Square overflow this unit | **HARD NO** (deliberate; separate blast radius) |
| Re-enable GPU gradients / unrelated | **n/a** |
| Merge authority | **Atlas + Pete** — not Prometheus |

### Recommended path (author option 2; Prometheus agrees)

1. Land **#130 at `e2dba9c`** (or equivalent tip that keeps crates≡master) as the instrument + P0b receipt PR.
2. Open a **new P1 PR** from residual commits `6c7c6f3..6b09f9d` (or cherry-pick onto develop/master post-#130).
3. Do **not** force-reset from this seat (Prometheus will not; Pete-only if chosen).

### Conditional alternate (merge tip as-is)

Only if Pete prefers one merge:

- Retitle/reframe PR body: **P0a + P0b receipt + P1 partial (rounded overflow clip)**.
- State explicitly that crates diverge from the P0b measurement SHA; the 1/26 receipt remains valid **at the measurement commit**, not as "this tip is instrument-only".
- Do not banner discrete 51→35 as an N/26 win.

Prometheus prefers the split.

---

## 5. Soft residuals (non-blocking for product CLEAR)

| Residual | Note |
|----------|------|
| Text/image ignore clip stack | Pre-existing; pin when a discrete class needs it |
| `render_borders` ignores radius | Verified mechanism; next discrete root candidate for `new_tab` border notches |
| Remaining discrete (~35 macOS) | Not one root — css-selectors / flex-positioning / sticky-scroll have no border |
| Vertical radius vs height resolution | `border_radius_px` historical scalar; ACCEPT match background painter |
| sticky-scroll % pixel loss | Layout y-drift exposure; ACCEPT keep |

---

## 6. Process pin (carry forward)

Stacking engine behavior onto an instrument PR that was already CLEARed as zero-crates **invalidates the tip as instrument** even when the product unit is sound. Outside-eye must re-measure tips that move past banked CLEARs — this tick is exactly that.

Symbol/cfg lesson from #133 still stands for removals; this residual is additive.

---

## 7. Owner actions

| Seat | Action |
|------|--------|
| **Atlas** | Prefer split: land #130 @ `e2dba9c` (or crates≡master tip); open P1 PR for overflow rounded clip. Else reframe #130 body before merge. Do not claim zero-engine at `6b09f9d`. |
| **Pete** | Optional: overrule sticky-scroll keep → literal stop-rule revert (cheap, two product commits). Prefer split vs one-merge. |
| **Argos** | Optional greps: `PushClipRounded` · square no-clip pin · positioned child inside clip · free-function wiring. |
| **Athena / Talos** | SAME_DEFECT audit when capacity — Win/Linux overflow clip emit sites. |
| **Prometheus next** | Outside-eye first *new* tip only (P1 split PR · tip past this CLEAR · other surfaces). Else **STOP**. Do not re-pin #130 CLEAR @ `e2dba9c` body · this residual CLEAR @ `6b09f9d` product · #110 · #59 · #33 HOLD · #11 HARD AMEND · community #6 · flip HARD NO unless measurement changes. |

---

## 8. Verdict summary

| Layer | Verdict |
|-------|---------|
| Product (overflow rounded clip mechanism + tests + macOS discrete delta) | **DESIGN CLEAR / APPROVE** |
| Packaging (#130 tip still "P0a instrument") | **HARD AMEND / SPLIT** (preferred) |
| Prior CLEAR @ `e2dba9c` | **STANDS** |
| Merge | Atlas (+ Pete if promote/path) — **not Prometheus** |

**No irreversible acts this seat:** no merge, force-push, spend, master write, null attend.

— prometheus · design R1 · 2026-08-10
