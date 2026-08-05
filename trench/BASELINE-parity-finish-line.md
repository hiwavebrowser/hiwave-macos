# Trench baseline — macOS Chrome-parity finish line

**Started:** 2026-08-04 · **Authorized by:** Pete (plan ratified 2026-08-04)
**Plan:** `docs/PARITY_FINISH_LINE_PLAN_2026-08-04.md` §4 (queue) · §5 (config)
**Branch:** `atlas/trench-parity-finish-line`

> This file was referenced by the campaign's night order before it existed.
> Created on night 1 to stop the reference dangling. It carries the metric and
> the reason the metric is not yet a number.

---

## The one metric

**`N/26 finish-line-green`** — cases passing the FULL conjunction, not a mean:

1. Geometry within **0.5px per box** vs `baselines/chrome-148/**/layout-rects.json`
2. Paint **≥ 99%** within `aa_tolerance: 5` (the single pinned constant, in
   `docs/VISUAL_DIFF_POLICY.md` — no second number may be introduced)
3. **Stable** across 3 iterations
4. **Zero** discrete structural failures (paint outside box, missing clip,
   wrong solid color) — these auto-fail regardless of percentage

```
BASELINE (2026-08-04):  UNMEASURABLE
```

**UNMEASURABLE is the honest reading, not a placeholder for a bad number.**
The oracle that would compute it does not exist yet. Any figure produced before
it does would be a mean-pixel-diff wearing a conjunction's clothes, which is the
exact substitution §1 of the plan documents: master's collapsed-shelf layout
scored 3.71% (pass) while the geometrically correct tree scored 33.87% (fail).
The metric preferred the broken layout.

Do not invent a number for this line. It gets one at **P0b**, from a dual-oracle
run on master, and not before.

### What blocked measurement, and what still does

| Blocker | State |
|---|---|
| RustKit `layout.json` had no join key — only `type`/`text`/`control_type`, while Chrome's rects are keyed by selector | **CLEARED** night 1 (P0a-0). 1593/1593 baseline selectors reproducible across all 26 cases. |
| Gate A (geometry) not implemented — `scripts/layout_oracle_gate.py` is a stub whose `extract_layout_from_rustkit` returns `None` | **CLEARED** night 2. Gate built, joined on the P0a-0 key, 14/14 mutation-checked. It has never seen a real RustKit capture — see below. |
| Gate A has no real RustKit input yet — every capture path needs a GPU adapter, and this trench seat is Linux with none | open. Not a code blocker: `parity-capture --dump-layout` already exists and CI is `macos-14`. The gate's join is proven against all 1593 committed Chrome boxes, but its *output on real engine data* is unobserved until it runs on macOS. |
| Gate B (paint tolerance + discrete-structural auto-fail) not implemented | open — P0a |
| Gate C (non-gating forensic board) not published | open — P0a |
| Stability never enforced at `pr_merge` (`stable:false` does not gate in `parity_gate.py`) | open — P0a |

---

## Order, and why

Per plan §4, worked strictly in sequence, one item per night, no skipping:

**P0a-0** export element identity → **P0a** build the four gates → **P0b** first
real `N/26` receipt → **P1** gradient/clip → **P2** grid/sticky → **P3** flex
residual → **P4** text advance widths → **P5** images + 10-site holdout board →
**P6** forms and paint-order/stacking.

P0a and P0b carry **zero engine behavior changes** so the metric delta is
attributable. A re-instrument PR that also "fixes something small" makes the
first real number unattributable and has to be redone.

---

## Fleet rule banked from this campaign's tasking (Talos, 2026-08-04)

**A blank Class-6 row is not a pass.** Asked to audit instrument integrity,
Talos reported *"I have no parity harness on this seat, so there is no
scoreboard here to catch lying"* — and returned NOT APPLICABLE instead of
green. A seat with no instrument reports the same "nothing wrong" as a seat
whose instrument is honest. In any cross-seat table, a seat without the
instrument reads **NOT APPLICABLE — NO INSTRUMENT**, never blank, never green.

He also withdrew his own advice to port the old parity harness to Linux, in
his words: *"I wanted a number so much that I proposed adopting one already
known to be false."* The old harness is not portable because it is the thing
being replaced.

## Stop rule (hard)

Any change that improves the metric while **any** oracle regresses on **any**
case is auto-reverted and logged in the digest as a mistake, with reasoning.
Improving a number while breaking correctness is the failure this campaign
exists to end.

## Banned from this loop

Mean-pixel-diff-only wins in PR prose (report geometry and paint as a pair, or
report UNMEASURABLE and why) · `dead_code` cleanups · MCP work · cross-platform
ports · corpus expansion · merges to master · force-pushes.

---

## Per-night receipt

Appended to `trench/digest-parity-finish-line.md`:

1. Metric before → after (`N/26`, or UNMEASURABLE + one line on what still blocks it)
2. The P-item worked, and whether it completed
3. Commits landed (SHA + one line each)
4. Mutation-check results for any new guard — a guard that stays green without
   its fix is decoration and does not count
5. At most three decisions needed from Pete, one sentence each
6. Anything that surprised you, especially a measurement that disagreed with an
   assumption
