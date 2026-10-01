# Closing the display gap with Chrome — ratified plan

**Status: RATIFIED by Pete, 2026-08-07 evening.** Approved in full: fleet
concentration with subsystem assignments, the livesuite lane, and all three
outstanding trench decisions (recorded in §5).

Derived from live-session evidence (2026-08-05 → 2026-08-07, sessions driven
by Pete's hands), not from theory. Companion to
`PARITY_FINISH_LINE_PLAN_2026-08-04.md` — this plan does not replace the
finish-line metric or its queue; it adds the live-site lane and re-tasks the
fleet onto one engine.

---

## 1. The structural decision: one engine, not three

The fleet does not have one engine on three platforms; it has **three copies
of the engine**. Every improvement currently costs 3× and the ports lag
(wedge fix ported by hand same-evening; ruby UA port still queued on two
platforms; Windows still carries the margin-shorthand defect).

**Ratified: macOS is the reference engine.** Windows and Linux freeze their
engine lanes — shells, input, and platform integration only. Engine work from
every builder lands in the macOS engine crates, which are pure Rust and
testable from any seat (headless + SwiftShader, proven on the trench seat
2026-08-07).

**Phase 2 (after the subsystem division proves out, not before): extract the
engine into a shared workspace** with three thin platform shells, retiring
the porting debt structurally. Do not start this migration during Phase 0/1.

Named risk: Windows/Linux shells rot during concentration. Mitigation: their
shell lanes stay alive at low intensity (input wiring both need anyway), and
Phase 2 retires the debt.

## 2. The ranked rendering classes (live evidence)

| Rank | Class | Live evidence | Size |
|---|---|---|---|
| 1 | Absolute/fixed positioning | Footers at TOP on google.com and x.com | M |
| 2 | Flex refinements (gap, alignment, min-content) | x.com overlapping buttons; Google search row | M |
| 3 | Inline `<svg>` elements | Missing X and Google logos (rustkit-svg exists; splice at layout like `<img>`) | S |
| 4 | Image sizing (`srcset`, CSS sizing, object-fit) | Tiny cropped Wikipedia globe | S–M |
| 5 | WebP decode | Every eBay photo (`Unsupported image format: WebP`) | **XS** |
| 6 | Text metrics / line breaking | Text overlap everywhere | L (campaign P4) |
| 7 | Grid + sticky | News-site skeletons | M (P2) |
| 8 | `@font-face` | Icon-font tofu. Measured ~¼ built: parse missing, loader orphaned, fetch hollow (see ORPHAN_LAW.md) | L |
| 9 | Stacking / paint order | Wrong-thing-on-top | M (P6) |

**Quick-wins bundle (days, transforms first impressions): #3 + #4 + #5.**
Logos and photos appearing is most of what a human notices.

## 3. The livesuite — measuring real pages

The gap between "13/13 micro cases pass" and "x.com looks broken" exists
because nothing measures real pages. New lane:

1. **~15–20 frozen real pages** spanning the classes (Wikipedia article,
   Google home, x.com login, eBay listing, news front page, GitHub, a docs
   site, …). Generic pages — the corpus lives in the repo; never Pete's
   personal tabs.
2. **Frozen means frozen**: Playwright captures full HTML + subresources into
   deterministic snapshots. No live network in any test run — bot walls and
   A/B tests make live fetches incomparable between runs.
3. Chrome renders the snapshot → layout-rects + screenshot baseline (same
   format as the 26-case corpus). RustKit renders the same bytes. **Same
   gates (A geometry / B paint), bigger teeth.**
4. **Attribution clustering** is the tool that turns "looks wrong" into work
   items: group Gate A's per-element deltas by CSS property class →
   "61 displaced elements, all `position:absolute` descendants" names the
   subsystem, with counts.
5. Non-gating board first (Gate C shape); promoted to gating only after
   stability, same discipline as everything else.
6. Runs nightly on the trench seat under SwiftShader (mechanics); receipts
   are macOS numbers only (§5.3).

## 4. Assignments (by subsystem, matched to demonstrated strengths)

| Seat | Lane | First unit |
|---|---|---|
| **Atlas** | Integration, input/product polish, promote gating, attribution clustering tool | Quick-wins bundle (WebP, inline SVG, image sizing); nav-buttons wire in flight |
| **Athena** | **Text metrics + `@font-face`** in macOS engine crates | `@font-face` first quarter: at-rule parse in rustkit-css producing the `FontFaceRule` the loader already consumes (her own measured tally is the spec) |
| **Talos** | **Absolute/fixed positioning + inline SVG** in macOS engine crates | Abs-pos containing-block correctness — the footer-at-top class |
| **Trench (nightly)** | Instrument lane: Gate C, livesuite harness + freezer, P0b first N/26 | Per §5 decisions below |
| **Prometheus** | Design pins + outside-eye R1, unchanged | R1 this plan |
| **Argos / Pollux** | R1 lanes, unchanged; cross-seat R1 weight increased for engine PRs from non-macOS builders | — |

Engine PRs from Athena/Talos land in hiwave-macos under the standing model
(self-merge to develop on green; promote gated). Their shells stay on their
own repos, low intensity.

## 5. Trench decisions — RATIFIED (were open since nights 1–4)

1. **Selector-form drift: PIN `capture_baseline.mjs` back to the committed
   form** (`div.card featured`), plus a test asserting script and committed
   baselines agree. Do not regenerate; 572 join keys and three gates ride on
   it.
2. **Gates A and B enter `parity.yml` ADVISORY-FIRST for one cycle** (print
   receipts, do not block), then flip blocking. The tightened stability bar
   ships the same way: one advisory cycle, then red-locks are the gate
   working.
3. **SwiftShader frames: YES for developing and validating instrument
   mechanics; NO for anything printed as a receipt.** The number that counts
   is produced on macOS. Every SwiftShader-derived figure is labeled as such.

## 6. Exit criteria — "near-completion" is a measurement

1. Finish-line conjunction on **N/26** (per the ratified campaign metric), and
2. Livesuite board ≥ threshold on all frozen sites (threshold set when the
   board has its first stable week), and
3. A live session in Pete's hands with **no verdict worse than "minor"**.

All three. Any banner short of all three names what is missing.

## 7. Sequencing

- **Phase 0 (now):** promote #110 lands; quick-wins bundle; trench P0b;
  gates into CI advisory.
- **Phase 1:** subsystem lanes run (§4); livesuite stood up; attribution
  clustering online.
- **Phase 2:** engine extraction to shared workspace; Windows/Linux shells
  reconnect to the one engine.

*Instances and provenance: live sessions 2026-08-05/06/07; trench digests
nights 1–4; ORPHAN_LAW.md; MACOS_PR104/PR110 promote R1s.*
