# Outside-eye R1 — hiwave-macos PR #150 (soft-wrap slice-0)

**Seat:** Prometheus (design) · **Date:** 2026-08-16 · **Tip:** `87c058a`  
**Base:** `master` @ `34ec5b4`  
**Verdict:** **DESIGN CLEAR / APPROVE** product @ `87c058a`  
**Plumbing-alone HARD NO:** **SATISFIED** (algorithm + parse + emergency fill; not parse-only)  
**Merge:** Atlas + Pete — **not** Prometheus. **Not** Athena auto-merge.  
**Land to master ahead of E0 #147+#148:** **HARD NO**.

Atlas noon digest n27 (seq 377) opened the tip and tasked Athena's shared-crate review lane. Argos seq 531 GREEN @ `87c058a` **CONFIRMED independently** (mechanism + local tests; not rubber-stamped). WPT 7/25→9/25 is **not** accepted as a product win.

---

## 1. Board (live this tick · 2026-08-16T20:15Z)

Queue rule: banked CLEARs stay banked; next = outside-eye first *new* tip. **#150 is that tip.**

| Surface | Tip / state |
|---------|-------------|
| macOS **#150** | tip **`87c058a`** · OPEN · MERGEABLE CLEAN · **NEW** · audit+script-guards+selector-key+pr-swarm×4+pr-aggregate **SUCCESS** · nightly **SKIPPED** |
| macOS **#147** | tip **`bf55a53`** · OPEN · CLEAR banked · vs master |
| macOS **#148** | tip **`8fb9792`** · OPEN · CLEAR banked · base #147 branch |
| macOS **#149** | tip **`8706566`** · OPEN · CLEAR banked · vs master |
| macOS master / develop | **`34ec5b4`** / **`c93614f`** (UNCHANGED) |
| Scheduled Parity Gate | **FAIL** [31939340261](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31939340261) · night-16 fossil **CONFIRMED** last tick |
| Win | open **#33 HOLD only** @ `d12321d` |
| Linux / umbrella / tank | open **zero** |
| community **#6** | OPEN · CLEAR @ **`f6b7891`** · GitHub tip **UNCHANGED** |
| Atlas last real | 2026-08-16T16:08Z seq **377** (n27 digest / #150 opened) |
| Argos last real | 2026-08-16T16:23Z seq **531** (CLEAR @ `87c058a`) |

Do not re-pin night-16 fossil · #147/#148/#149 CLEAR · SHARE 3.45% · S0(a) PARK. Seed / raise budget / PR-CI-as-proof / +2.06 as paint / ink engine remain **HARD NO**.

---

## 2. Independent ground (worktree `/tmp/hiwave-pr150-r1` @ `87c058a`)

Local `hiwave-macos` is dirty on `atlas/guard-runner-worst-first`. Review used a **detached worktree** of the committed tip only.

| Unit | Result |
|------|--------|
| Scope | 11 files · +1135/−73 · **crates real** (css/engine/layout/text) + `wpt_tier1.py` + forensics + `last-run.json` |
| crates tree SHA | **`0852bd33` ≠ master `3e9b1a9c`** — engine change is real |
| Cargo.toml tree SHA | **`70dcfd0c` ≡ master** BYTE-IDENTICAL |
| merge-tree vs master | **CLEAN** write-tree `ba73b31` |
| PR CI | audit · script-guards · selector-key · pr-swarm 0–3 · pr-aggregate **SUCCESS** |
| nightly-aggregate | **SKIPPED** — not a lock-break / not a seed receipt |

### Mechanism (read, not author-prose)

| Claim | Result |
|-------|--------|
| `word-break` has a producer | **YES** — engine parse sets `ComputedStyle.word_break` (break-all / keep-all / break-word / normal) |
| `overflow-wrap` / `word-wrap` computed | **YES** — new `OverflowWrap` enum + inheritance + layout plumbing. `LineBreaker` is no longer hardcoded `OverflowWrap::Normal` |
| `word-break: break-all` is a real opportunity | **YES** — `break_opportunities` now emits grapheme `Allowed` ops (css-text-3 §4.2). `find_line_break` consumes them on the **normal** fill path |
| Emergency arm fills the line | **YES** — `break_offset==0` + `may_break_mid_word` takes the longest grapheme prefix that fits, min one. One-grapheme-per-line is gone |
| `overflow-wrap` last-resort (not break-all) | **YES** — `break_opportunities` still ignores `overflow_wrap`; `allows_emergency_breaks()` gates the emergency arm. Correct CSS layering |
| `word-break: break-word` own value | **YES** — computes `WordBreak::BreakWord`, not an `overflow_wrap` write-through. Layout treats it as emergency (`BreakAll \| BreakWord`). Survives a later `overflow-wrap: normal` |
| Plumbing-alone HARD NO | **SATISFIED** — parse reaches a breaker that now *does* the work |

### Local tests (this seat, worktree @ `87c058a`)

| Suite | Result |
|-------|--------|
| `cargo test -p rustkit-text --lib` | **68/68 PASS** |
| `cargo test -p rustkit-layout --lib` | **260/260 PASS** |
| `test_word_break_and_line_break_reach_the_line_breaker` | **PASS** (0.26s, compositor live; 4–12 line ceiling held) |
| `test_ch_width_resolves_against_the_zero_glyph` | **PASS** |
| `test_ch_unit_tokenizer_is_narrow` | **PASS** |

Engine test pins break-all / line-break:anywhere / overflow-wrap:anywhere / overflow-wrap:break-word / word-wrap:break-word against a 34-char word in a 40px box. It does **not** pin `word-break: break-word`.

### WPT receipt (committed `last-run.json` — **not** a tip measurement)

| Fact | Measured |
|------|----------|
| `hiwave_git_sha` | **`4fdc96f`** — two commits **behind** tip (`a70453e` + `87c058a` not in this run) |
| Rate on that SHA | 9/30 pass (0.36) vs master last-run 7/30 (0.28) |
| The +2 | `overflow-wrap-001` and `overflow-wrap-002` FAIL→PASS |
| Those two | listed in **`suspect_passes`** (`blocked_by: @font-face unimplemented`) |
| `break-word-overflow-wrap-interactions` | already **PASS on master** (Atlas's own "coincidental identical-wrong" class) |
| `line-break-anywhere-001/002` | still **FAIL** (0.30% / 0.25%) |

Harness contract on this repo: a PASS whose declared web font never loaded is the dangerous direction. **Do not quote 9/25.**

---

## 3. Soft residuals (do not block CLEAR)

1. **`line-break: anywhere` writes `overflow_wrap`** — wrong layer (last-resort, not a normal opportunity) and order-fragile against a later `overflow-wrap: normal`. Same class as the alias bug they just fixed. `line-break-anywhere-001/002` still FAIL. **Do not claim the line-break axis is done.**
2. **`emergency_breaks()`** is still only called from its own unit test. Layout inlined the fitted-prefix loop. Correctness lives in `wrap_segment`, not in that method.
3. **ch-units ride-along** (`448ba2e`) — separate CSS Values feature, ledgered shorthand/longhand replay hole. ACCEPT this residual. **Do not banner "ch works."**
4. **No new rustkit-text / rustkit-layout unit test** for grapheme opportunities or fitted emergency. Load-bearing pin is the engine integration test, which omits `word-break: break-word`.
5. **O(n²) reshape** of every grapheme prefix — author-ledgered. ACCEPT.
6. **WIP-bank commit chain** + `trench/wpt/last-run.json` mid-stack SHA — hygiene only.

---

## 4. Rulings

| Item | Ruling |
|------|--------|
| #150 product (slice-0 algorithm) | **DESIGN CLEAR / APPROVE** @ `87c058a` |
| Plumbing-alone HARD NO (Aug-12 pin) | **SATISFIED** — do not re-open |
| Quote 7/25→9/25 or campaign 26/26 @ 6.6% as a win | **HARD NO** |
| Quote suspect_passes as proof wrap is measured | **HARD NO** |
| Claim `line-break` axis finished | **HARD NO** |
| Auto-merge on Athena approval | **HARD NO** — Pete master gate stands |
| Land #150 → **master** before E0 #147+#148 | **HARD NO** — resets N on `34ec5b4`; mixes engine into the fossil window |
| Preferred land | **develop** after E0 freeze, or Pete explicitly reorders |
| E0 #147→retarget #148→#148→#149→freeze | **STANDS** |
| Seed / raise budget / ink engine / PR-CI-as-proof | **HARD NO** |
| Merge | **Atlas + Pete** — not Prometheus |

---

## 5. Tasking

- **Atlas:** hold Pete-gate on E0. Do **not** auto-merge #150 to master. Retarget #150 → develop unless Pete reorders in writing. Optional: add `word-break: break-word` to the engine wrap test; refresh `last-run.json` at tip SHA. Do not cite 9/25.
- **Athena:** shared-crate review lane ACK. Your CLEAR is product, not a merge authorization.
- **Argos:** seq 531 GREEN stands. Optional: glance after retarget. Smoke first **scheduled** after #147+#148 on master; download ≠ `8856038965` / 30813903898.
- **Pete:** master go still required for E0. No re-ping from this seat. #150 is not a reason to jump the nightly unlock.
- **Prometheus next:** outside-eye first *new* tip, or first-green scheduled after E0 land. Else **STOP**.

No irreversible: no merge / force-push / spend / master write / null attend.
