# Outside-eye R1 — hiwave-macos PR #110 tip residual `a4c0053` (post-`7f0f84d` CLEAR)

**Seat:** Prometheus (design only)  
**Date:** 2026-08-08  
**PR:** https://github.com/hiwavebrowser/hiwave-macos/pull/110  
**Tip under review:** `a4c00532b494f61d09b088ca003bda39e03d7eac` ≡ `origin/develop`  
**Prior banked CLEAR:** `7f0f84d` (stands for that SHA only)  
**Master at measure:** `44389f1`  
**Verdict:** **DESIGN CLEAR / APPROVE** promote at tip `a4c0053`

---

## 0. Queue rule

Banked CLEARs stay banked. Next unit = outside-eye first *new* tip.  
Live tip **moved** `7f0f84d` → `a4c0053` (+2 commits: product `88873f3` + merge #127).  
Argos already posted R1 GREEN for this tip move; this document is independent design ground, not a re-stamp of Argos prose.

## 1. Live board (this tick)

| Surface | Tip / state |
|---------|-------------|
| macOS **#110** | tip **`a4c0053`** · OPEN · MERGEABLE · audit+swarm×4+aggregate **SUCCESS** · **NEW residual** |
| macOS master / develop | **`44389f1`** / **`a4c0053`** |
| macOS open | **#110 only** |
| Win | open **#33 HOLD only** @ `d12321d` · develop `b79c008` |
| Linux **#59** | OPEN @ `7ad1eb0` — tip moved by **merge of banked #58 only** (not a new product residual) |
| Linux **#58** | **MERGED** @ `1f073949` |
| community **#6** | OPEN @ `f6b7891` · R1 CLEAR **banked** |
| tank | open **zero** · main `85ce800` |
| umbrella **#11** | OPEN · HARD AMEND banked @ `0b5993d` |

## 2. Scope of the tip residual (only)

```
ahead of 7f0f84d: 2 commits
  88873f39  fix: finish the object-fit default — #125 fixed two sites of four
  a4c00532  Merge pull request #127 from hiwavebrowser/atlas/object-fit-remaining

files:
  crates/rustkit-image/src/lib.rs   +1/−1  (#[default] Contain → Fill)
  crates/rustkit-layout/src/lib.rs  +19/−1 (#[default] + pin test)
```

No engine rewrite, no keys path, no font loader, no scripts/last-run. Thin residual that closes a Prometheus SOFT pin from the `7f0f84d` R1.

## 3. Parent defect (CONFIRMED at `7f0f84d`)

Prior R1 banked #125 product path CLEAR (ComputedStyle initial `"fill"` + layout keyword `_ => Fill`) but named **SOFT**:

> enum `#[default]` still **Contain** on both `ObjectFit` enums

Independent re-read at parent:

| Enum | Parent `#[default]` |
|------|---------------------|
| `rustkit-layout::ObjectFit` | **Contain** |
| `rustkit-image::ObjectFit` | **Contain** |

Why this mattered: `ObjectFit::from_css` unknown arm is `_ => ObjectFit::default()` (layout). Any path constructing via `Default` (or unknown keyword via that arm) letterboxed under Contain while the visible stylesheet initial path already said fill. Partial fix = more dangerous than open bug.

## 4. Tip fix (CONFIRMED at `a4c0053`)

### Four-site census (CSS Images 3 §5.5 initial = `fill`)

| # | Site | Tip state |
|---|------|-----------|
| 1 | `ComputedStyle` initial `object_fit: "fill"` | **FILL** (from #125; still present) |
| 2 | layout paint-path keyword `_ => ObjectFit::Fill` | **FILL** (from #125; still present) |
| 3 | `rustkit-layout::ObjectFit` `#[default]` | **Fill** (this residual) |
| 4 | `rustkit-image::ObjectFit` `#[default]` | **Fill** (this residual) |

### T-RED pin

```rust
// layout: object_fit_default_tests::object_fit_derived_default_is_fill_not_contain
assert_eq!(ObjectFit::default(), ObjectFit::Fill);
```

Local measure (worktree `/tmp/hiwave-pr110-tip-a4c0053` @ `a4c0053`):

```
test object_fit_default_tests::object_fit_derived_default_is_fill_not_contain ... ok
```

CI: audit + pr-swarm×4 + pr-aggregate **SUCCESS** (completed ~2026-08-08T01:48Z).

### merge-tree

`git merge-tree --write-tree origin/master origin/develop` → clean write-tree SHA (no conflict markers for product residual).

## 5. Rulings

| Item | Ruling |
|------|--------|
| #110 promote residual @ `a4c0053` | **DESIGN CLEAR / APPROVE** |
| Prior CLEAR @ `7f0f84d` / `a60ecac` | **STAND** for those SHAs only |
| #127 closes SOFT `#[default]` pin | **CLEAR** |
| Banner "typing verified e2e" | **HARD NO** until Pete live fingers receipt |
| Banner "webfonts work" | **HARD NO** (Q1 parse only; FontLoader never-instantiated pin separate) |
| Dual `ObjectFit` enums (layout + image) | **ACCEPT** for this residual — consolidation is not #127 scope |
| image crate lacks Default pin test | **SOFT nit** — layout pin covers the dangerous `from_css → default()` arm; optional thin test later |
| Merge | **Atlas + Pete** — not Prometheus |

## 6. Prior body still on the promote (not re-opened)

Units banked CLEAR under earlier tips remain in the cumulative develop→master diff. This residual does **not** re-litigate:

- keys-follow-focus design CLEAR / e2e UNMEASURED  
- nav buttons RustKit nav_* CLEAR  
- WebP decode path CLEAR  
- @font-face Q1 parse CLEAR / load_font hollow / FontLoader never-instantiated (Athena Q2 + Prometheus ownership pin)  
- srcset widest-w / densest-x subset CLEAR  

## 7. Out of unit / handoffs

| Seat | Action |
|------|--------|
| **Atlas + Pete** | Land #110 when Pete go |
| **Pete** | Key-delivery live receipt (still HARD gate on product banner) |
| **Athena** | FontLoader Unit A (Engine owns `Arc<FontLoader>`, partition-keyed from day one) — pin already on exchange |
| **Talos** | Abs-pos containing-block (S2) first tip when capacity |
| **Argos** | Tip GREEN already stands; no re-pin required from this note |
| **Prometheus next** | Outside-eye first *new* tip only. Do **not** re-pin this CLEAR @ `a4c0053`. Linux #59 tip move is #58 merge only — not a new residual unless product SHA moves past `b662494` body. |

## 8. Irreversible acts

**None from this seat:** no merge, force-push, spend, master write, null attend, delete.

---

— Prometheus · grind tick 2026-08-08 · one unit · stop
