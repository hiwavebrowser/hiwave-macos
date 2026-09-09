# S0(a) ink SHARE — 14.7% retired; engine PR HARD NO (2026-08-14)

> **Status:** SHARE card (Prometheus design only). Gate **NOT SATISFIED**. No engine PR.  
> **Audience:** Atlas (do not open stem/gamma/smoothing; optional calibration card only), Argos (do not treat 14.7% as live), Athena (not your lane), Pete (no action).  
> **Exists in service of:** closing the “kinda bold / 14.7% more ink” claim under the blessed SHARE gate before anyone spends a night on coverage.  
> **Companions:** blessed pin seq 542 · ranking `tank/docs/NEXT_STRATEGIC_SLICE_2026-08-13.md` · E0 implement `MACOS_E0_NIGHTLY_FOSSIL_LOCK_IMPLEMENT_2026-08-13.md` (order stands; not re-argued).  
> **Does not:** open an engine PR · re-pin E0 · seed · flip A/B · quote 1/26 or +5.55% as a product win · re-open subpixel flip.

---

## 0. Live board (re-measured 2026-08-14T01:30Z)

Empty design queue. No new open tip. Measurement vs last-tick E0 brief: **UNCHANGED**.

| Surface | Live truth |
|---------|------------|
| macOS open | **zero** |
| master / develop | **`34ec5b4`** / **`c93614f`** |
| Scheduled Parity Gate | still [31690222033](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31690222033) **FAIL** (swarms green) |
| E0 / E0a / seed PR | **none opened** |
| Win #33 HOLD @ `d12321d` · Linux open zero · community #6 CLEAR @ `f6b7891` · tank zero | unchanged |

E0 (break nightly fossil-lock) remains Atlas’s execution first. This tick does **not** re-pin that brief. This is the first *new* product research the queue named: **S0(a) ink SHARE**.

---

## 1. Verdict (one screen)

| Item | Ruling |
|------|--------|
| Atlas seq 353 **14.7%** (`9,380,241 / 8,181,701` on develop `7f59b35`) as SHARE | **RETIRED / HARD NO** |
| Open stem-darkening · global gamma · font-smoothing · CT↔Skia engine PR | **HARD NO** |
| Uniform “we paint more ink” as one-knob explanation | **FALSIFIED** (mixed sign on lawful boxes) |
| SHARE floor (~10% of the paint-residual class) | **NOT SATISFIED** — **3.45%** of corpus Gate-B outside-px |
| Lawful ink_ratio (clean leaf text, #146 `8726553`) | **1.0555 (+5.55%)** — measurement, not permission |
| Quote +5.55% / +9.01% / 14.7% as a campaign win | **HARD NO** |
| Geometry leftover on article-typography (115 px below-fold cascade · highlight `x−392`) | **NOT INK** — do not fold into a paint PR |
| Case-level geometry-green + paint-red (`bg-solid` / `pseudo-classes` / `specificity`) | **NOT text-ink** (fills on `bg-solid` match Chrome to 0–1 RGB) |
| S0(c) CT↔Skia | stays **closed** until (a) residue exists *and* a card isolates shape |
| E0 → E0a → E0b ranking | **STANDS** — not re-ranked this tick |

**Proposal line:** *Do not build an ink engine unit. The 14.7% number is not SHARE. On the first post-S0(b) receipt, attributable leaf text is only +5.55% ink and 3.45% of corpus paint-outside — below the floor — and the per-box signs disagree, so a global stem/gamma knob is the wrong next PR. Atlas: E0 still first. Optional: one calibration card if you want a mechanism name on file. Else park S0(a).*

---

## 2. Why 14.7% was never SHARE

Blessed pin (seq 542) required, for ink:

1. Unit + mechanism  
2. SHARE on **geometry-green text only**  
3. Falsifier  
4. Impact floor ≈ 10% of that residual class  

Atlas seq 353 measured **whole-page** darkness on develop **`7f59b35`** (2026-08-08), *before* #137/#143/#145/#146. Gate A on that page was still a large vertical residual (sweep saturated at ±3 px). Whole-page Σink mixes:

- displaced boxes (paint of *something else* at Chrome’s rect),
- red descendants inside green ancestors (the highlight span),
- true coverage/weight.

Pete’s “kinda bold” still corroborates that *some* text looks heavy. It does not certify a 14.7% SHARE or a mechanism.

---

## 3. Independent ground (this tick)

Post-S0(b) receipt: PR **#146** swarm [31647877127](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31647877127) · head **`8726553`** · merged → develop **`c93614f`**.

| Artifact | Id | Used for |
|----------|----|----------|
| `parity-oracle` | `9161576050` | Gate A / Gate B / finish-line |
| `parity-shard-1` | `9161514396` | `article-typography/1280x800/iter-1/capture/frame.ppm` |
| Chrome SoT | `baselines/chrome-148/websuite/article-typography/baseline.png` | same 1280×800 RGB |

Gate B `outside_tolerance_px` on this case **reproduced locally: 114,687 / 1,024,000 (11.20%)**. Finish-line on this receipt: geometry **4/26** · paint **1/26** · discrete **25/26** · conjunction **1/26**.

Ink proxy on opaque RGB (no alpha in either frame):

```
darkness = 255 − luma_Rec601
ink_ratio = Σ darkness_RK / Σ darkness_Chrome
```

That is a coverage/weight proxy, **not** an alpha histogram. Named so nobody launders it into “we measured gamma.”

### 3.1 Case-level paint residual after S0(b)

Corpus Gate-B outside-px on #146: **2,169,418**.

| Slice | Outside-px | Share of corpus |
|-------|-----------:|----------------:|
| All 22 geometry-**red** cases | 2,106,182 | **97.09%** |
| 3 geometry-**green** + paint-red cases (`bg-solid` 8,445 · `pseudo-classes` 26,284 · `specificity` 28,507) | 63,236 | **2.91%** |
| `article-typography` (case is geometry-red) | 114,687 | 5.29% |

The only geometry-green paint-red *cases* are selector/fill fixtures, not article text. On `bg-solid`, inset means of the named/hex/rgb/red boxes are **0.0–1.0 RGB** vs Chrome. That residual is not a fill-gamma smoking gun and is **not** S0(a) text ink.

### 3.2 article-typography geometry (so the 14.7% page is still mixed)

| | n |
|--|--:|
| Chrome boxes compared | 62 |
| Unique failing selectors | 39 |
| Geometry-green selectors | **23** (matches Gate B `discrete_examined`) |
| y-failures | 31 · median **115.3 px** · max 129.6 · only 5 ≤ 2 px |
| Below-fold cascade | `article` height −115 · columns y −130 |

The 115 px class is leftover **geometry**, not ink. Highlight span `p:nth-of-type(3) > span.highlight` is `x−392`. Those pixels cannot enter an ink SHARE.

### 3.3 Lawful ink (descendant-clean, then leaf)

A geometry-green **parent** is not a lawful crop if it contains a geometry-red child. `p:nth-of-type(3)` is green; its highlight child is not. That parent alone is **+29.1%** ink and 33,765 outside-px — **laundered layout**.

| Mask (viewport union, no double-count) | ink_ratio | excess | outside-px | of this case | of corpus |
|----------------------------------------|----------:|-------:|-----------:|-------------:|----------:|
| Whole 1280×800 viewport | 1.0901 | +9.01% | 114,687 | 100% | 5.29% |
| All 23 green boxes (contaminated) | 1.1122 | +11.22% | 113,632 | 99.08% | 5.24% |
| Green, **no red descendant** | 1.0634 | +6.34% | 79,867 | 69.7% | 3.68% |
| **Clean leaf textish** (the SHARE number) | **1.0555** | **+5.55%** | **74,751** | 65.2% | **3.45%** |

Clean leaf textish visible on this viewport:

| Selector | ink_ratio | read |
|----------|----------:|------|
| `header > h1` | 1.020 | +2.0% |
| `header > p.subtitle` | 1.181 | **+18.1%** |
| `p.lead` | 1.132 | +13.2% |
| `p.drop-cap` | 1.167 | +16.7% |
| `h2:nth-of-type(1)` | 0.786 | **−21.4%** |
| `blockquote > cite` | 0.940 | −6.1% |
| five `p.meta > span` | 0.58–0.70 | **−30 to −42%** |

Per-box excess darkness on those leaves is **~92% interior / 8% 1-px fringe**. That is *compatible* with heavier stems, but it is **not** a histogram, and the **signs disagree across boxes**. A single global stem-darkening or gamma knob predicts same-sign movement. It did not happen.

`p.lead` / `p.drop-cap` are still large paragraph rects (background + text). They are the least-dirty crop this receipt allows. They are **not** a calibration card.

---

## 4. SHARE fields (mandatory gate, filled)

1. **Unit.** Geometry-green, descendant-clean, leaf text boxes on `article-typography@1280x800`, #146 `8726553` vs chrome-148. Named leaves: `h1`, `p.subtitle`, `p.lead`, `p.drop-cap`, first `h2`, `cite`, five meta spans.  
2. **Mechanism (hypothesis, not confirmed).** CoreText coverage/weight vs Chrome/Skia on the same box — *or* per-run font-weight/color misses (the meta spans are *lighter*). Not isolated.  
3. **SHARE.** **74,751 / 2,169,418 = 3.45%** of corpus Gate-B outside-px. ink_ratio **1.0555**. Primary metric = corpus paint-outside on lawful boxes.  
4. **Falsifier (already fired for the one-knob story).** A global stem/gamma change must move the meta spans and the first `h2` in the **same direction** as `p.lead`. They move opposite.  
5. **Impact floor.** **Miss.** 3.45% < ~10%. Do not build. Record and park.

Systemic-ink objection (honest): this is one case. If every geometry-green text box in the corpus carried +5.55% ink, SHARE could rise. Counter: (a) 97% of corpus outside-px sit on geometry-**red** cases and are **unattributable**; (b) signs already disagree *inside* the one text page we can read; (c) the three geometry-green paint-red cases are not text-ink. Raising SHARE by assuming the unmeasurable 97% is the same class as 14.7% was.

---

## 5. Calibration card (optional Atlas follow-up — not a PR)

Only if someone still wants a mechanism name on file. **Not** required to park S0(a).

HTML, one viewport, same-seat, reset injected (the #145 lesson):

- Card A: `#808080` field, no text (negative control — fill must match, as `bg-solid` already does).  
- Card B: one glyph `H` (and `o`, `i`) at 16 / 24 / 48 px, `font-weight: 400`, color `#111` on `#fff`, line-height 1, geometry forced to match Chrome (or skip if Gate A fails).  
- Report: `ink_ratio`, 16-bin alpha-or-darkness histogram, interior vs 1-px fringe, **same-seat** Chrome + RustKit PNGs, `engine_sha`.  
- Then pick **at most one** of: gamma · stem-darkening · smoothing. No multi-hypothesis engine PR.  
- If card SHARE of a paint-red *case* is still < 10%, park.

Do **not** run the card on `article-typography` as a whole page.

---

## 6. What this seat will not do

No merge, force-push, spend, master write, seed, null attend, or engine branch. Atlas owns E0 and any card. Argos smokes scheduled E0, not this research.

---

## 7. Seat actions

| Seat | Do | Do not |
|------|----|--------|
| **Atlas** | E0 vs **master** (two yaml lines; prior brief stands). After that E0a. Optional card §5. | Ink engine PR · quote 14.7% · seed today |
| **Argos** | Smoke next **scheduled** run after E0 lands | Re-R1 #146 · treat 14.7% as live |
| **Athena** | Hold Win keyboard FALLEN | Take text-ink |
| **Pete** | None this unit | Forced raw A/B flip |
| **Prometheus** | Outside-eye first *new* tip only (E0 PR · E0a · seed · card receipt) | Re-pin this SHARE or the E0 implement brief unless the SHA/artifact moves |

---

## 8. Artifacts

- This file (uncommitted macos docs lane — Atlas PR lane if desired)  
- Exchange doorbell-note this tick  
- Scratch (not in-repo): `/tmp/prom-s0a-146/` oracle + shard extracts  

— prometheus (Grok / design seat, scheduled grind) · 2026-08-14 · no merge/attend/seed
