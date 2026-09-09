# Outside-eye R1 — hiwave-macos PR #118 (ruby UA → inline)

**Seat:** Prometheus (design only)  
**Date:** 2026-08-07  
**Tip:** `4c08a19dbc79af846a1baa583aebb6cf5c52fd0a`  
**Branch:** `atlas/ruby-inline` → `master`  
**Verdict:** **DESIGN CLEAR / APPROVE merge** @ `4c08a19`  
**Merge lane:** Atlas — **not Prometheus**

---

## Board (re-measured this tick)

| Surface | Tip / state |
|---------|-------------|
| macOS **#118** | tip **`4c08a19`** · OPEN · MERGEABLE **CLEAN** · **NEW** · audit+swarm×4+aggregate **SUCCESS** |
| macOS **#117** | **MERGED** @ `d4db9a7` (prior CLEAR banked @ `f31e42f`) |
| macOS **#110** | OPEN · CLEAR banked @ `a60ecac` · tip UNCHANGED |
| macOS master / develop | **`d4db9a7`** / **`a60ecac`** |
| macOS **#112** / **#99** | **MERGED** / **CLOSED** SUPERSEDE |
| Win | open **#33 HOLD only** @ `d12321d` · develop `67ec265` · master `f0c2f5a` |
| Linux **#59** / **#58** | OPEN · CLEAR banked @ `b662494` / `387a8ee` · tips UNCHANGED |
| umbrella #11 | OPEN · HARD AMEND banked · tip moved to `0b5993d` (docs only; not this unit) |
| community / tank | open **zero** · tank main **`85ce800`** |
| CI | Actions dispatching macos; re-arm still Pete word |

Queue rule satisfied: banked CLEARs stay banked; **#118 is first *new* tip** past #117 CLEAR.

---

## Independent ground

Worktrees: `/tmp/hiwave-pr118-r1` @ `4c08a19` · `/tmp/hiwave-pr118-master` @ `d4db9a7`.

| Check | Result |
|-------|--------|
| Scope | `crates/rustkit-engine/src/lib.rs` only (+92/−0) · **0 scripts / 0 WPT last-run rewrite** |
| Parent | `da8f0fc` (pre-#117) · tip **not** descendant of current master |
| merge-base | `da8f0fc` |
| merge-tree vs live master | **CLEAN** (GitHub `mergeStateStatus: CLEAN` · `mergeable: MERGEABLE`) · engine hunk applies without conflict (#117 was scripts-only) |
| CI | audit + pr-swarm×4 + pr-aggregate **SUCCESS** (run ~31152469786) |

### Master defect (CONFIRMED)

1. `Display` enum has `#[default] Block` (`rustkit-css`).
2. `ComputedStyle::new()` fills explicit fields then `..Default::default()` → **display starts Block**.
3. UA tag match in `compute_style_for_element` has arms for span/a/strong/… but **zero** arms for `ruby` / `rb` / `rt` / `rtc` / `rp`.
4. Catch-all is `_ => {}` (~L2703 master) → unknown tags keep **Block**.
5. WPT `css-inline/empty-span-size-002` (third_party WPT pin) uses bare empty `<ruby class="inline">` (and one bordered `<ruby class="has-height">`). Under master, those rubies are **block-level full-width** bars, not inline boxes on a line.

Master `trench/wpt/last-run.json` (post-#117 seed-30 @ master `d4db9a7` / recorded `hiwave_git_sha` da8f0fc era stamp on merge tip):  
`css-inline/empty-span-size-002` → **FAIL** · `diff_pct: 0.6658` · `diff_pixels: 3196` · `blocked_by: null` (honest unattributed head named since #112 R1).

### Tip fix (CONFIRMED)

```text
"ruby" | "rb" | "rt" | "rtc" => Display::Inline
"rp"                         => Display::None
```

Spec ground (accepted as stated + consistent with engine limits):

- css-ruby-1 §2.1: engines **without** ruby layout must treat ruby display values as **inline**.
- Chrome UA: `display:ruby` (inline-level) for the box; `rp` hidden.
- RustKit has **no** `Display::Ruby` variant and no ruby layout — mapping to `Inline` / `None` is the correct **fallback**, not a full ruby implementation.

### Load-bearing test

`test_ruby_ua_display_inline`: real engine path — empty bordered `<ruby style="border: 3px solid">` must:

- compute `Display::Inline`
- build `BoxType::Inline`
- border-box width **&lt; 100** (must not fill 800px containing block)

Soft nit: test constructs `Compositor::new()` and **returns early** if GPU unavailable (same pattern as sibling engine tests). CI Mac runners have GPU path; local headless may skip. Does not weaken the design of the fix.

### Campaign / corpus safety

Grep of `websuite` + `fixtures` + `cases` under tip/master trees for `<ruby` / family tags: **ZERO hits**. Campaign board claim “untouched” stands for content presence. Full suite re-run is Atlas/CI lane (swarm SUCCESS on this PR).

### Residual honesty (ACCEPT stated — out of this unit)

Author receipt (not re-run this seat; last-run on tip branch is **stale seed-14** and correctly **not** rewritten by an engine-only PR):

| Metric | Before (block ruby) | After (inline fallback) |
|--------|---------------------|-------------------------|
| empty-span-size-002 diff | 0.6658% | **0.0108%** (52 px) claimed |
| exact-match (MAX_DIFF=0) | FAIL | **still FAIL** |

Named residual (do **not** expand #118):

1. **outline paint unimplemented** — test uses `outline: 1px solid` on flanking empty rubies; ref emulates with `border-right: 2px` on a span.
2. **empty borderless inline box participation** — empty inlines without border may still not produce boxes.

That is a paint / empty-inline lane, not a UA-display lane.

### Cross-fleet SAME_DEFECT

| Tree | `ruby` arms in rustkit-engine |
|------|-------------------------------|
| macOS tip | **YES** (this PR) |
| Windows local engine | **0** hits → **SAME_DEFECT** (Athena unit when capacity) |
| Linux local engine | **0** hits → **SAME_DEFECT** (Talos unit when capacity) |

Not a merge gate for #118.

### Soft nits (non-blocking)

| Nit | Note |
|-----|------|
| PR body “WPT Tier-1 unchanged **6/9**” | Stale vs live master post-#117 (**7/25** seed-30). Product fix is independent; scrub body on merge or leave — **do not** re-pin rate claims without re-run under master harness |
| Tip last-run still seed-14 | Expected for engine-only PR; **do not** ship a fake last-run refresh without a real seed-30 re-run |
| GPU skip in unit test | Soft; CI covered |
| Full ruby layout / Display::Ruby | **NO** this PR |

---

## Rulings

| Item | Ruling |
|------|--------|
| #118 product | **DESIGN CLEAR / APPROVE merge** @ `4c08a19` |
| Master defect (UA catch-all → Block for ruby family) | **CONFIRMED** |
| css-ruby-1 §2.1 → Inline fallback | **CLEAR** |
| rp → None | **CLEAR** (Chrome UA parity for fallback parens) |
| Quote empty-span as PASS / Tier-1 rate win | **HARD NO** until seed-30 re-run under master harness; residual still FAIL at exact match |
| Expand outline paint / empty-inline | **NO** this PR |
| Expand full ruby layout | **NO** |
| Merge | **Atlas** — not Prometheus |
| #110 CLEAR / #117 MERGED / banked others | **unchanged** |

---

## Seat asks

**Atlas**

1. Land #118 → master when green (merge commit; base can stay master).
2. Optional: re-run WPT Tier-1 under seed-30 harness; update `last-run.json` only if measured.
3. Do not banner empty-span as fixed-to-green; name residual (outline + empty-inline).
4. #110 promote still Pete go (separate unit).
5. Optional follow-up PRs: outline paint · empty borderless inline boxes · Win/Linux ruby UA port.

**Athena / Talos**

- Windows / Linux: thin UA arms only when a local receipt needs them (SAME_DEFECT).

**Argos**

- Optional greps: `"ruby" \| "rb"` arms present; only `rustkit-engine` in diff; no last-run rewrite; CI SUCCESS.

**Pete**

- Master go already exercised on #117; #118 is product fix on shared engine — Atlas land when process allows.
- CI re-arm still open word.

---

## What this seat did **not** do

- No merge / force-push / master write / spend  
- No null attend  
- No last-run rewrite  
- No engine implement beyond review  

---

## Durable artifacts this tick

- This file: `hiwave-macos/docs/MACOS_PR118_RUBY_INLINE_R1_2026-08-07.md`
- WORK_QUEUE tick entry
- Exchange doorbell-note (schema:1)

**Prometheus next:** outside-eye first *new* tip only (key-delivery if opened · Win tip past HOLD · #59 tip past CLEAR · post-#118 residual). Else **STOP** — do not re-pin #118 CLEAR · #117 MERGED · #110 CLEAR · banked set unless measurement changes.
