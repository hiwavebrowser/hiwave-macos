# Trench baseline — MCP engine exports

**Started:** 2026-07-29 · **Authorized by:** Pete ("good trench candidate. Do it.")
**Plan:** `HIWAVE_MCP_PLAN.md` §11 · **Branch:** `atlas/trench-mcp-exports`

> **Trench 1 closed 2026-08-02 at 4 of 4** (nights 0–4). Its metric — how many
> Tier-1 MCP reads the engine can answer — is **complete and is no longer the
> live metric**. It is kept below as the record and the model for how a metric
> is pinned. **Trench 2 is live; its metric is at the bottom of this file, and
> that is the number a working night moves.** Pete, 2026-08-03: *"Point at new
> metrics, eat off the next chunk of the elephant."*
>
> **If your instructions still describe the metric as "how many of the four
> Tier-1 MCP reads the engine can answer" and tell you to stop at 4 of 4, they
> are STALE** — that is trench 1, which is finished. The nightly prompt is
> stored outside this repo and could not be edited from inside a session (it
> was created via the API; agents may only edit routines they created), so it
> still names trench 1 until Pete updates it. This file is the binding one:
> work trench 2, and report `N of 12`. Do not stop on trench 1's stop
> condition.

---

## Trench 1 (CLOSED) — the one metric

**Engine export coverage — how many of the four Tier-1 MCP reads the engine can
ANSWER.**

```
BASELINE (2026-07-29):  1 of 4
```

| Tier-1 tool | Engine support | Proof |
|---|:--:|---|
| `hiwave_layout` | **YES** | `rustkit-engine::export_layout_json` (lib.rs:4608); asserted in `crates/hiwave-mcp/smoke.py` — `.hero border_box = 432.0x152.0` |
| `hiwave_display_list` | no | — |
| `hiwave_style` | no | — |
| `hiwave_diff(case, stage, reference)` | no | — |

### Why this metric and not another

It is **not** lines of code, commits landed, or "work done on exports." Every
one of those measures motion rather than capability, and this repo has spent
two days finding instruments that reported motion as progress (#65's parity
gate publishing 73.36 for a 6.75 tree; #69's banner naming the best cases
"worst").

"Can an agent ask this question and get an answer" is checkable, binary, and
proven per tool by a smoke assertion. A tool counts as answerable **only** when
`crates/hiwave-mcp/smoke.py` asserts a real value from it — not when the export
compiles.

---

## Stop condition

Whichever comes first:

1. **4 of 4**, each with a passing smoke assertion.
2. **Two consecutive nights with no tool moving no → yes.**

Clause 2 is the important one. An export that needs engine surgery will stall,
and a loop that cannot distinguish stalling from working is the same failure as
a gate that cannot go red. Two dry nights ends it — with a funeral note, not
silence.

---

## Order, and why

1. **`hiwave_display_list`** — first. Paint is where every HiWave bug this year
   actually lived: the ADVANCE CONTRACT, gradient axis routing, colour emoji,
   the `rustkit-svg` break. §1's whole argument is that an agent should see the
   layout/paint boundary rather than infer it from pixels, and layout is already
   answerable — so this is the missing half of the pair.
2. **`hiwave_style`** — computed value **plus winning rule and origin**. The
   cascade is where "parsed but dead" bugs hide; seven dead behaviours were
   found by hand this year.
3. **`hiwave_diff`** — **last**, always. It consumes the other three. Building
   it earlier means stubbing what it consumes, which is inventing a baseline.

---

## Hard scope limits

- **No Windows or Linux work.** Per Pete's ruling and §10.3: there is one MCP
  design, and a second implementation would be a signal of a baseline gap, not
  a platform requirement. Windows needs the engine *exports*, not a second MCP.
- **Do not touch the parity harness.** It has taken two honesty fixes in two
  days (#65, #69). A third hand in it this week is asking for a fourth.
- **No CI gate from this loop.** Prometheus's R1 stands: the MCP must not become
  the reason the workspace build/test gate keeps slipping.
- **Coexists with the parity/WPT trench.** That loop is live on this repo (#69
  was "night block 20"). Disjoint surfaces, separate branch, and a later start
  time so two agents are not committing to the same repo at once.

---

## Per-night receipt

Appended to `trench/digest.md`. Not "worked on display list":

1. Which tool moved no → yes, **or explicitly NONE**.
2. The smoke assertion that proves it — run, with output pasted.
3. What it still cannot answer, named rather than omitted.
4. At most three decisions needed from Pete.

---

# Trench 2 — computed-style answer coverage (LIVE)

**Started:** 2026-08-03 · **Authorized by:** Pete ("Point at new metrics, eat
off the next chunk of the elephant.") · Same branch, same receipt discipline.

## The one metric

**How many properties in the DIAGNOSIS SET `hiwave_style` can answer, where an
answer is a value the engine COMPUTED and a provenance that does not lie.**

```
BASELINE (2026-08-03):  0 of 12
```

### The diagnosis set, and why these twelve

Not "all of CSS" and not "whatever is easy to serialize". These are the
properties that decide **where pixels land**, chosen because the parity backlog
already blames them — text metrics account for ~59% of the remaining diff, and
an agent cannot currently ask which line-height or family the cascade handed to
layout.

| # | Property | Why it is in the set |
|---|---|---|
| 1 | `line-height` | Text metrics, the largest single parity bucket |
| 2 | `font-family` | Which face was chosen decides every advance |
| 3 | `text-align` | Horizontal placement of every line box |
| 4 | `font-style` | Synthetic vs real italic changes advances |
| 5 | `letter-spacing` | Directly perturbs the ADVANCE CONTRACT |
| 6 | `white-space` | Decides whether a line breaks at all |
| 7 | `border-top-width` | Set almost only via the `border` shorthand |
| 8 | `border-top-color` | Same, and paint reads it |
| 9 | `box-sizing` | Silently redefines what `width` means |
| 10 | `position` | Decides whether the box is in flow |
| 11 | `overflow-x` | Decides whether a clip is pushed |
| 12 | `opacity` | Decides whether a layer is created |

## What counts as answered

A property counts **only** when all three hold:

1. `crates/hiwave-mcp/smoke.py` asserts its computed value, run and pasted.
2. The asserted value is one the engine **computed**, not echoed from the
   declaration text — the fixture must make value ≠ authored text (a resolved
   multiplier, a shorthand expansion, an inherited value, a unit conversion).
   `line-height: 1.5` on a 20px element asserting `"1.5"` proves nothing;
   asserting `"30px"` proves the engine resolved it.
3. **The reported provenance does not lie.** If a shorthand set it, the winner
   cites that shorthand. If it was inherited, the origin says `inherited` and
   not `user-agent-or-initial`. **A property whose reported value can differ
   from the value layout used does not count at all** — a tool that disagrees
   with the engine is worse than a gap, so the gap is the honest answer.

Clause 3 is the one that will keep the count low, and that is deliberate.

## Stop condition

Whichever comes first:

1. **12 of 12**, each with a passing smoke assertion.
2. **Two consecutive nights with no property moving no → yes.**

Same as trench 1: two dry nights ends it with a funeral note, not silence.

## Order, and why

1. **The text group first** (`line-height`, `font-family`, `text-align`,
   `font-style`, `letter-spacing`, `white-space`) — parity blames text for the
   majority of the remaining diff, so this is where an answer is worth most.
2. **Then the shorthand group** (`border-top-width`, `border-top-color`) —
   these are the ones that force shorthand→longhand provenance, which trench 1
   named as the one place the output can currently mislead.
3. **Then the box group** (`box-sizing`, `position`, `overflow-x`, `opacity`)
   — cheapest, and least likely to surface engine surgery, so it is the tail
   rather than the head.

## Hard scope limits

Unchanged from trench 1, and they still bind: no parity harness, no Windows or
Linux port work, no CI workflow changes, no refactoring unrelated code, and do
not "improve" exports that already have passing assertions. One addition:

- **Do not fix the engine to make a property answerable.** If the cascade is
  wrong, or the value the tool sees is not the value layout uses, that is a
  FINDING — report it, pin it with a tripwire assertion, and leave the property
  uncounted. Rendering changes belong to the parity corpus, not to an export
  loop. Trench 1 held this line twice (`!important`, night 2) and it is why
  those findings are trustworthy.
