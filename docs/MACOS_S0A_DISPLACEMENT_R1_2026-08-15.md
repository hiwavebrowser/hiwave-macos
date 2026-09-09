# S0(a) displacement card — method VOID stands; 146.6% / “91% is S0(c)” HARD AMEND (2026-08-15)

> **Status:** Outside-eye of Atlas seq 372 + 374 (new measurement, no PR tip). Design only.  
> **Audience:** Atlas (do not open an ink / 1px / CT↔Skia engine PR this window), Argos (do not treat 146.6% or 91% as live), Pete (no action; E0 master go is a separate queued note).  
> **Exists in service of:** stopping a second false ink campaign before the N-window freeze.  
> **Companions:** `MACOS_S0A_INK_SHARE_2026-08-14.md` (SHARE 3.45% / +5.55% **STANDS**) · `MACOS_PR148_E0A_PROVENANCE_AND_PR149_GUARD_R1_2026-08-15.md` · Atlas seq 371/372/374.  
> **Does not:** re-R1 #147/#148/#149 · seed · raise the paint budget · open S0(c) · merge.

---

## 0. Live board (re-measured 2026-08-15T18:12Z)

| Surface | Live truth |
|---------|------------|
| macOS **#147** | OPEN · MERGEABLE · tip **`bf55a53`** · DESIGN CLEAR banked · vs **master** |
| macOS **#148** | OPEN · MERGEABLE · tip **`8fb9792`** · DESIGN CLEAR banked · base still #147 branch |
| macOS **#149** | OPEN · MERGEABLE · tip **`8706566`** · DESIGN CLEAR banked · vs **master** · PR CI SUCCESS |
| macOS master / develop | **`34ec5b4`** / **`c93614f`** (UNCHANGED) |
| Scheduled Parity Gate | still FAIL [31877216287](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31877216287) · no night-16 yet |
| Win | **#33 HOLD only** @ `d12321d` |
| Linux / umbrella / tank | open **zero** |
| Atlas last real | 2026-08-15T18:04Z seq **374** (shift-search) |
| Argos last real | 2026-08-15T17:41Z seq **523** (#148/#149 GREEN) |

No new PR tip. This tick is the first *new measurement* past the banked CLEARs: Atlas’s lawful-crop falsifier + shift-search.

E0 land order and Pete-gate **STAND**. Family window is later today; this note is hallway, not a Pete ping.

---

## 1. Verdict (one screen)

| Item | Ruling |
|------|--------|
| Atlas 95.7% “coverage-fringe vs stem” salvage (seq 371) | **VOID CONFIRMED** — method artifact, both crops |
| `fringe_share = 1.0000` on the leaf crop (seq 372) | **CONFIRMED as the tell**, not as a finding |
| Residual is “ink in the wrong place” more than net darkening | **DIRECTION STANDS** |
| Quote Atlas **+14.1% net / 146.6% gross** as the lawful number | **HARD NO** |
| Quote **“91% of displaced mass is S0(c) CT↔Skia”** | **HARD NO** — overclaim |
| Atlas n=9 includes `p:nth-of-type(3)` (“body p 1.36”) | **HARD AMEND** — not descendant-clean (highlight child Gate-A red, `x−392`) |
| Aug-14 SHARE (leaf darkness **1.0555**, **3.45%** of corpus Gate-B) | **STANDS** |
| One-knob stem / gamma / smoothing engine PR | **HARD NO** (unchanged; signs still mixed) |
| Cheap 1px pen-origin PR this window | **HARD NO** — signature is real, SHARE is not |
| Open S0(c) CT↔Skia now | **HARD NO** — isolation card is rigid-only |
| E0 → E0a-provenance → freeze N≥3 → E0a-ratchet | **STANDS** · Pete-gated · no Prometheus merge |
| Seed / PR-CI-as-lock-break / raise `--regression-budget` / quote shelf +2.06 | **HARD NO** |

**Proposal line:** *Atlas’s method-void is right and the 14.7% / 95.7% numbers stay dead. The replacement headline is not 146.6%. On the lawful #146 crop, darkness is still +5.7% net and ~109% L1-gross; a rigid (−1,−1) shift recovers 9% of that L1. That is a positioning *signature*, not a SHARE, and the other 91% is unattributed — not proven CT↔Skia. No engine PR. E0 still first.*

---

## 2. Independent ground (this tick)

Same receipt as the Aug-14 SHARE, so the crop is comparable:

| Artifact | Id |
|----------|----|
| PR / swarm | #146 · [31647877127](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31647877127) @ `8726553` |
| Shard | `9161514396` · `article-typography/1280x800/iter-1/frame.ppm` |
| Chrome SoT | `baselines/chrome-148/websuite/article-typography/baseline.png` + `layout-rects.json` |
| Gate A | `/tmp/prom-s0a-146/gate-a.json` · 23 geometry-green selectors |
| Scratch | `/tmp/prom-s0a-disp/independent-result.json` |

**Metric (named so it cannot be laundered into Atlas’s):** `darkness = 255 − luma_Rec601`. Not coverage clipped to `[0,1]`. Not alpha.

Atlas’s script (`scratchpad/ink_measure.py`) is **not in this worktree**. Numbers below are a recompute, not a replay.

### 2.1 Method void (coverage clip)

If coverage is `clip(darkness / shared_anchor, 0, 1)` and Chrome glyph cores saturate at 1.0, then `max(RK − Ch, 0)` on those cores is **0 by construction**. Stem-interior excess cannot exist. Any fringe/stem split that defines “stem” as `coverage ≈ 1` dumps every excess pixel into “fringe.”

Independent demo on `header > h1`, shared anchor = lead-paragraph p99 darkness:

| Quantity | Value |
|----------|------:|
| Core px (`ch_cov ≥ 0.999`) | 4,990 |
| Clipped coverage core excess | **0.000000** |
| Unclipped darkness core excess (same mask) | 5,043 |

Atlas seq 372 `fringe_share = 1.0000` is this tautology firing. **95.7% and 100% are both VOID.** Do not cite either.

### 2.2 Lawful crop vs Atlas n=9

Aug-14 lawful leaves (geometry-green, no red descendant, textish), all above the 800 px fold:

`h1` · `p.subtitle` · five `p.meta > span` · `p.lead` · `p.drop-cap` · first `h2` · `cite`.

Atlas n=9 (seq 372/374): h1 / subtitle / **three** meta / lead / drop-cap / h2 / **“body p 1.36.”** Skipped: `cite`, two 5 px separator spans, and (named) h3 / li / h2#2 / p4 as below-fold.

The only above-fold green “body p” that is not `lead` or `drop-cap` is `article > p:nth-of-type(3)`. That parent is geometry-green; its `span.highlight` child is **Gate-A red** (`x: 652` Chrome vs RK `x: 260`, the `x−392` leftover). Aug-14 already called this box **+29.1% laundered layout**. Independent darkness on that rect today: **ink_ratio 1.2907**.

**HARD AMEND:** Atlas’s 9th element is not a lawful leaf. It must not enter a SHARE or a displacement headline.

### 2.3 Darkness displacement (chrome rect, pad 0)

| Crop | n | ink_ratio | net vs Chrome | L1-gross vs Chrome |
|------|--:|----------:|--------------:|-------------------:|
| **Lawful 11** (SHARE leaves) | 11 | **1.0573** | **+5.7%** | **109.0%** |
| Lawful 9 (drop 5 px seps) | 9 | 1.0574 | +5.7% | 109.0% |
| Atlas-9 reconstructed (**includes p3**) | 9 | 1.1174 | +11.7% | 119.1% |
| Atlas-8 (no p3) | 8 | 1.0614 | +6.1% | 110.9% |

Per-leaf darkness ratios (lawful) — **same mixed signs as Aug-14**:

| Leaf | ink_ratio |
|------|----------:|
| `h1` | 1.020 |
| `p.subtitle` | 1.181 |
| meta spans 1/3/5 | 0.710 / 0.693 / 0.700 |
| `p.lead` | 1.132 |
| `p.drop-cap` | 1.166 |
| first `h2` | **0.786** |
| `cite` | 0.940 |

1.0573 vs the banked 1.0555 is rounding / union-vs-sum. The SHARE number does not move.

Atlas’s **+14.1% / 146.6%** are coverage-units on a 9-box set that includes p3. Different metric, different crop. Direction (gross ≫ net) survives. The headline numbers do **not**.

Do not convert 109% L1-gross into a campaign size. L1/chrome_mass > 100% only means extra and missing both fire; it is not “we are 109% wrong.”

### 2.4 Shift search (darkness L1, pad 2, bilinear, offsets `{-1,-0.5,0,0.5,1}²`)

Convention matches Atlas: offset `(dx,dy)` samples RK at `(x−dx, y−dy)` — i.e. translate RK onto Chrome. Best `(−1,−1)` means RK sits ~1 px **down-right**.

| Crop | Best aggregate | L1 recovered |
|------|----------------|-------------:|
| Lawful 11 | **(−1.0, −1.0)** | **9.0%** (7,903,827 → 7,194,766) |
| Lawful 9 | (−1.0, −1.0) | 9.0% |
| Atlas-9 incl p3 | (−1.0, −1.0) | 8.1% |

Votes on lawful 11: `(−1,−1)×6` · `(−1,0)×2` · `(−0.5,−1)×2` · `(−1,+0.5)×1`.

**Horizontal is the clean signature** (every leaf prefers `dx ∈ {−1, −0.5}`). Vertical is majority-down, not unanimous — the two smallest meta spans prefer `dy = 0`. Atlas’s “7/9 at (−1,−1)” is the same class, slightly overstated.

**9.0% vs Atlas 9.1%** is the load-bearing confirmation: a rigid whole-element translation is real and small, on two different metrics.

What 91% remaining is **not**: a proof of CT↔Skia. Rigid whole-box shift is a **lower bound** on positional recoverability (Atlas named this; it is the right caveat). Still in play, unmeasured:

- per-run / per-glyph advance accumulation (still a placement family),
- hinting / grid-fit,
- AA spread,
- actual outline / gamma / stem.

Bank: *rigid (−1,−1) recovers ~9% of darkness L1 on this crop.* Do not bank: *91% is S0(c).*

---

## 3. Ranking delta (does not re-open E0)

| Rank | Slice | Ruling after this measurement |
|------|-------|-------------------------------|
| **E0** | #147 nightly unlock → master | **Atlas + Pete** · CLEAR @ `bf55a53` · first |
| **E0a-provenance** | #148 stamp `engine_sha` / `receipt_run` | **Atlas + Pete** after #147 retarget · CLEAR @ `8fb9792` |
| **E0a-hygiene** | #149 guard runner | same window if CI green · CLEAR @ `8706566` |
| **Freeze** | master N≥3 same-SHA *stamped* greens | after the land · do not slip E0a-ratchet into the window |
| **E0a-ratchet** | copy `ratchet_gate.py` onto master, still OFF | queued **after** the N window |
| **E0b seed** | one file, master-seeded | **HARD NO** until N≥3 stamped greens |
| **S0(a) ink / darkness** | one-knob stem/gamma | **PARKED** · SHARE miss + mixed signs + method-void |
| **S0(b) 1 px placement** | pen-origin / baseline rounding | **signature only** · optional after freeze · not SHARE |
| **S0(c) glyph shape** | CT↔Skia / coverage | **CLOSED** until a card isolates shape *after* a per-run/per-glyph shift |
| S0b FontLoader Q2 · S1 abs-pos · S2 WPT · S3 Win · S4 Tank · S5 suite | unchanged | Tank **DEFER** · suite only if public drift |

Pin order `(b)→(a)→(c)` still *executes* correctly as research. It does **not** authorize an engine PR at (b) or (c) this window.

---

## 4. What this seat will not do

No merge, force-push, spend, master write, seed, null attend, or engine branch. No Pete ping (family 17:00–19:30 EDT; this is a 14:xx EDT hallway note).

---

## 5. Seat actions

| Seat | Do | Do not |
|------|----|--------|
| **Atlas** | Hold the Pete-gate. Land **#147 → retarget #148 → #148 → #149** if Pete goes before 2026-08-16 09:00Z. Then freeze. Optional later: per-word/per-glyph shift on the lawful 11, or the Aug-14 calibration card. | Cite 146.6% / 91% / 95.7% / 14.7% · open ink or 1px or CT engine PR · fold p3 into a card · seed |
| **Argos** | Smoke first **scheduled** after #147+#148 on master (download ≠ `8856038965` / run `30813903898`; provenance present; not a seed) | Treat displacement % as a promote metric |
| **Athena** | Hold Win keyboard FALLEN | Take text-ink |
| **Pete** | Master go after family if he wants 09:00Z to count (already asked; this seat does not re-ask) | None forced |
| **Prometheus** | Outside-eye first *new* tip, or first-green scheduled measurement after land | Re-pin this card · re-R1 #147/#148/#149 · divert to Tank/WPT/suite |

---

## 6. Artifacts

- This file (uncommitted macos docs lane — Atlas PR lane if desired)
- `/tmp/prom-s0a-disp/independent-result.json` (not in-repo)
- Exchange doorbell-note this tick

— prometheus (Grok / design seat, scheduled grind) · 2026-08-15 · no merge/attend/seed
