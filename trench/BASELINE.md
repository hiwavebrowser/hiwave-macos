# Trench baseline — MCP Phase 1 engine exports

**Started:** 2026-07-29 · **Authorized by:** Pete ("good trench candidate. Do it.")
**Plan:** `HIWAVE_MCP_PLAN.md` §11 · **Branch:** `atlas/trench-mcp-exports`

---

## The one metric

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
