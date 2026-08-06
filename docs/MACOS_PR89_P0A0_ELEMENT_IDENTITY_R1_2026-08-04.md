# macOS PR #89 — P0a-0 element identity export — outside-eye R1

**Date:** 2026-08-04  
**Reviewer:** Prometheus (Grok / headless grind)  
**Tip:** `97dcc51670450bedeb1e11bfdae8bb46d9c6831e`  
**Branch:** `atlas/trench-parity-finish-line`  
**Base:** `origin/master` `d5df733ee4065964cf50388bfe6eec477da61dea`  
**PR:** https://github.com/hiwavebrowser/hiwave-macos/pull/89  
**Verdict:** **DESIGN CLEAR / APPROVE merge**

---

## 1. Queue context

Prior tick banked #86 gradient-clip CLEAR. Queue next = first *new* tip.

Live board this tick:

| Surface | Tip / state |
|---------|-------------|
| macOS **#89** | tip **`97dcc51`** · OPEN · MERGEABLE · **NEW** · CI audit+swarm×4+aggregate **SUCCESS** |
| macOS **#88** | `3272e35` · trench campaign docs · OPEN (docs residual; not this unit) |
| macOS **#86** | `f7d8918` · CLEAR banked · still OPEN (Atlas merge lane) |
| Win | open = **#33 HOLD only** · master paint stack advanced (`cc4caff` box-shadow MERGED) |
| Linux | open **empty** |
| umbrella #11 | still OPEN · HARD AMEND banked |
| tank | open **zero** |
| community #2 | pin-provenance docs-only · lower priority |

#89 is the first product unit of the parity finish-line (plan §4 P0a-0). Independent of #86 paint and of #88 campaign baseline docs.

---

## 2. Independent ground

Worktree: local `hiwave-macos` @ exact tip `97dcc51` (≡ `origin/pr/89`).

### 2.1 Scope

| Path | Δ |
|------|---|
| `crates/rustkit-engine/src/lib.rs` | +621 / −111 (produce + export + tests) |
| `crates/rustkit-layout/src/lib.rs` | +47 / −0 (`ElementIdentity`, `set_identity` lockstep) |
| `trench/BASELINE-parity-finish-line.md` | +91 (campaign baseline) |
| `trench/digest-parity-finish-line.md` | +110 (night-1 digest) |

No renderer / CSS parse / layout algorithm changes outside identity production and export.

### 2.2 Master defect — CONFIRMED

| Check | Master (`d5df733`) |
|-------|---------------------|
| `LayoutBox::element_id` | field exists · default `None` |
| `set_element_id` product callers | **ZERO** (`git grep` product paths empty) |
| `layout.json` join fields | type / text / control_type only — **no selector / tag / element_id** |
| Engine comment | already admits "element_id is always None" (~L5993 on master) |
| `intrinsic_cache` live callers | **none** outside its own unit tests |

Plan assumption "just export the field" was **false**. Identity must be *produced* during tree construction. Accidental upside: a never-populated field cannot change layout → no-behavior-change is **provable**, not merely inspected.

### 2.3 Tip arm

| Mechanism | Ground |
|-----------|--------|
| `ElementIdentity { element_id, tag, selector }` | layout crate · `Option` on box |
| `set_identity` | sets `element_id` + `identity` **together** (lockstep) |
| Production site | `build_layout_from_parent_style_and_path` when `selector_path` non-empty |
| Path root | starts at **body** (Chrome capture skips `html`) |
| Document-order ids | `Cell<usize>` shared across one build |
| Anonymous / text / pseudo | **no** `set_identity` — `identity: None` (pseudo via `create_pseudo_element` confirmed) |
| Export | `layout_box_to_json` inserts three fields only when `identity.is_some()`; early-return image/form paths still go through outer insert |

### 2.4 Join-key fidelity (load-bearing quirks)

Mirrored from **committed** `baselines/chrome-148` capture form, not from the *current* `capture_baseline.mjs`:

| Quirk | Tip | Baseline measure |
|-------|-----|------------------|
| Multi-class | `div.card featured` (raw className after one `.`, space intact) | **572** space-form multiclass selectors in chrome-148 |
| `:nth-of-type` | only when same-tag sibling total **> 1** | unit + corpus |
| id short-circuit | reported `#id`; path for descendants still structural | unit |
| body report | path `body` → reported `html > body` | unit |
| Foreign content | `svg` / `math` and descendants drop class (Chrome `typeof className === 'string'` fails on `SVGAnimatedString`) | corpus caught shelf.svg |

**Corpus (local this tick):**  
`every_chrome_baseline_selector_is_reproduced_on_the_real_corpus` → **1593 / 1593** selectors across **26** cases · **PASS**.

Direction of the corpus check is correct for the oracle: every Chrome baseline selector must be *findable* in RustKit (Chrome ⊆ RustKit). RustKit may produce extra selectors Chrome skipped (zero-size / non-rendered); that is not a join failure.

### 2.5 Tests (local)

| Filter | Result |
|--------|--------|
| `selector` (identity + related) | **7 passed** |
| `export_emits` | **2 passed** |
| `every_chrome` | **1 passed** · 1593/1593 |
| CI | audit · pr-swarm ×4 · pr-aggregate **SUCCESS** |

Mutation suite claimed in PR body (multi-class join, nth threshold, id short-circuit, export omit/unconditional, foreign-class) is coherent with the test matrix; Atlas independently mutated nth `>1`→`>2` and saw corpus + unit red.

### 2.6 merge-tree

`merge-tree` tip vs `origin/master`: **0 conflict markers**.

### 2.7 Capture-script drift (measured, not re-debated)

Live `tools/parity_oracle/capture_baseline.mjs` `getSelector`:

```js
sel += '.' + classes.slice(0, 2).join('.');
```

→ would emit `div.card.featured` (dotted, max two classes). Committed baselines use space form (`div.card featured`). **Regenerating baselines today would silently break 572 join keys.** PR correctly mirrors baselines, not the drifted script.

### 2.8 Soft residual

| Item | Note |
|------|------|
| `export_emits_identity_for_image_and_form_control_boxes` | Title claims form_control; body only asserts image path. Form path is structurally identical (outer insert after early return) — **SOFT nit**, non-blocking. |
| Metric still UNMEASURABLE | Correct: P0a-0 unlocks join; P0a/P0b still owed before N/26. Do not invent a number. |

---

## 3. Rulings

| Item | Ruling |
|------|--------|
| #89 product (produce + export identity) | **DESIGN CLEAR / APPROVE merge** @ `97dcc51` |
| Join key = committed chrome-148 form | **CLEAR** (quirks deliberate) |
| Multi-class space form | **ACCEPT** (join key ≠ CSS selector) |
| Foreign-content class drop | **CLEAR** (SVG trap; corpus-proven) |
| `Option` identity for anon/text/pseudo | **CLEAR** (load-bearing exclude) |
| No layout behavior change | **CLEAR** (provable via prior zero callers) |
| Metric honesty (still UNMEASURABLE) | **CLEAR** |
| Capture script drift | **DESIGN RECOMMEND:** pin script back to committed form + regression test — **prefer over** regenerating 572 keys. Pete still owns if he wants full regen later. |
| Dead `intrinsic_cache` | **LEAVE** this PR; do not delete or "fix" in P0a-0. Separate residual if Pete wants cleanup. |
| Form-control export assert | **SOFT nit** — optional follow-up assert |
| #88 campaign docs | **independent** — not a merge prereq for #89 |
| #86 gradient clip | prior CLEAR **stands** — separate Atlas land |
| Merge | **Atlas** — **not Prometheus** |

---

## 4. Seat actions

| Seat | Action |
|------|--------|
| **Atlas** | Land #89 when green (optional rebase onto master). Optional: form_control export assert; capture-script pin follow-up. |
| **Pete** | (1) capture script pin vs baseline regen — design leans **pin**. (2) keep or delete dead intrinsic_cache — leave for later. |
| **Prometheus** | No re-pin #89 unless tip moves or scope expands past identity. Next = first *new* tip after land / move. |

---

## 5. Not done (this seat)

- No merge / force-push / master write  
- No null attend  
- No spend / irreversible  
- Docs left uncommitted on macos lane for Atlas ownership  

---

_one_liner: #89 P0a-0 element identity DESIGN CLEAR @ 97dcc51 — 1593/1593 join keys, no-behavior-change provable, Atlas merges._
