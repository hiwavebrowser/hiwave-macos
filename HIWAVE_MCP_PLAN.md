# HiWave MCP — engine-native tooling for agents

**Status:** PROPOSED · high-priority backlog, next improvement
**Raised by:** Pete, 2026-07-28
**Written by:** Atlas, 2026-07-28

Pete's framing: *"We need a HiWave-MCP, so you can natively test and display
HiWave across the spectrum of architectures. Similar to how you use
chrome-mcp."*

Agreed, and it is closer to done than it looks. One reframe below, which is
the whole reason I think this is worth doing first rather than eventually.

---

## 1. The reframe: introspection beats screenshots

The obvious build is "chrome-mcp, but HiWave" — navigate, screenshot, click.
That is the *less* valuable half, and it is the half that mostly already
exists.

What chrome-mcp actually gives an agent is a **black box with a camera**. I can
see what Chrome painted and read its DOM. I cannot ask Chrome *why* it painted
that, because I do not own Chrome.

We own HiWave. An agent driving HiWave can read the engine's own intermediate
state — the computed style, the layout tree, the display list — and compare
each stage against the Chrome oracle we already run. That is strictly more
than chrome-mcp can ever offer, and it is aimed directly at the work that is
actually blocked.

Concretely, the parity loop today is:

```
run parity → get a number (about: 16.17) → guess which stage is wrong
           → patch → rerun → get another number
```

The 2026-07-27 trench digest ends with `about` at 16.17 and the residual
described as "text wall" — a guess, made because attribution is expensive.
The loop we want is:

```
ask the engine what it computed for this box
ask the oracle what Chrome computed for the same box
read the delta → the stage that diverged names itself
```

That is not a nicer camera. That is a different debugging modality, and it is
why I would sequence introspection first and screenshots second.

---

## 2. What already exists (roughly 70% of the read path)

This is not a greenfield project. Grounded, not assumed:

| Capability | Where | State |
|---|---|---|
| Headless offscreen render | `rustkit-compositor` `HeadlessState`, `headless` feature | built (`docs/HEADLESS_MODE_IMPLEMENTATION.md`) |
| Load URL/file, render at a viewport, emit PPM | `crates/parity-capture/src/main.rs` (`--width/--height/--dump-layout`) | built, and the one thing CI compiles |
| **Layout tree as JSON** | `rustkit-engine::export_layout_json` (lib.rs:4608) | built |
| Frame capture | `rustkit-engine::capture_frame` (lib.rs:4560) | built |
| Chrome oracle / baselines | `tools/parity_oracle/deterministic.mjs`, `parity-baseline/captures/*` | built |
| Swarm/parallel runner | `scripts/parity_swarm.py`, `parity_test.py` | built |

So the MCP is a **server over an engine we already drive headlessly**, not a
new rendering path.

### The gaps that are the actual work

1. **No display-list export.** Layout JSON only. But paint is where this
   year's bugs have lived — the ADVANCE CONTRACT, gradient axis routing,
   colour emoji, the `rustkit-svg` break. Layout can be right while paint is
   wrong, and today an agent cannot see the boundary.
2. **No style/cascade query.** "What did you compute for `.hero h1`, and which
   rule won?" is unanswerable without recompiling with `eprintln!`.
3. **No console / JS error channel** out of `rustkit-js`.
4. **One-shot process model.** `parity-capture` spawns, renders, exits. Fine
   for a batch gate, wrong for an interactive agent that wants twenty queries
   against one loaded page.
5. **No single call that returns "HiWave says X, Chrome says Y."** Both halves
   exist; nothing joins them.

---

## 3. Proposed tool surface

Ordered by value, not by resemblance to chrome-mcp.

**Tier 1 — introspection (the reason to build this)**

- `hiwave_layout(selector?)` — computed layout tree, or the subtree for one
  selector: box, content/padding/border rects, baseline, line boxes.
- `hiwave_display_list(selector?)` — the paint commands for a region, with the
  fields that keep biting us (`advances`, `ascent`, gradient stops, z-order).
- `hiwave_style(selector, property?)` — computed value **plus winning rule and
  origin**. The cascade is where "parsed but dead" bugs hide; seven dead
  behaviours have been found by hand this year.
- `hiwave_diff(case, stage)` — HiWave vs the Chrome oracle at a chosen stage
  (`style` | `layout` | `paint` | `pixels`), returning the first stage that
  diverges. This is the attribution tool.

**Tier 2 — drive**

- `hiwave_open(url | html | fixture)`, `hiwave_viewport(w, h, dpr)`,
  `hiwave_reload()`
- `hiwave_console()` — JS errors and engine warnings
- `hiwave_eval(js)` — gated behind a flag; useful, and the easiest way to
  wedge the engine

**Tier 3 — display**

- `hiwave_screenshot(region?)` — PNG for the agent to look at
- `hiwave_compare(case)` — side-by-side + heatmap, the artefacts
  `parity-baseline/diffs/` already produces

---

## 4. Architecture — and one thing I would not do

**Do not make this a new repo.**

Per the working rules, a new repo owes a sentence: *"This exists in service
of ___."* HiWave fills that blank cleanly, so the gate passes. But it should
still not be a repo, for a technical reason: the MCP's value comes from
calling `rustkit-engine` **in-process**. A separate repo means a version
boundary, an IPC hop, and the engine's internals frozen behind a published
API — which
is precisely the thing we want unfrozen. Display lists and cascade state are
not a stable public API and should not become one.

So: **`crates/hiwave-mcp`, inside the existing workspace**, depending on
`rustkit-engine` directly, exactly as `parity-capture` does.

### "Across the spectrum of architectures"

This falls out of the existing structure rather than needing new machinery:

- `hiwave-macos` and `hiwave-windows` are separate repos with parallel crate
  trees, so each builds its own `hiwave-mcp` — same tool surface, native
  engine per platform, no cross-compilation.
- The fleet already has a seat per platform. Each seat runs its own
  `hiwave-mcp` against its own build, and results ride the exchange the way
  parity numbers already do.
- **CORRECTION (Atlas, same day).** An earlier draft of this section said
  `hiwave-windows` last moved 2026-01-04 and would need reviving. That was
  read off a **stale local clone**; the remote is at `63091a0` (2026-07-22),
  **67 commits** ahead of it, with Athena actively landing ported parity work
  (W53 text-align, radial gradients, CSS Grid, W55 form controls, W56
  line-height). It carries the full crate tree **including
  `crates/parity-capture`**. There is also a `hiwavebrowser/hiwave-linux`
  repo, and Talos is walking the Linux platform-glue path now.

  This changes the recommendation rather than a detail of it. Windows is not
  a revival project — it is a live parallel tree with the same capture crate
  the macOS MCP would wrap, so the port is genuinely near-free rather than
  gated on waking a dead repo. Phase 3 can start as soon as Phase 1's export
  paths are designed, and Athena/Pollux are already in that tree.

  Recorded because this is the second time in one day I read repo state from
  a stale local checkout instead of the remote. The rule is now: **`git fetch`
  and quote `origin/`, or do not make the claim.**

---

## 5. Phasing

**Phase 0 — prove the read path** *(small)*
Wrap what exists: `hiwave_open`, `hiwave_screenshot`, `hiwave_layout` over
`export_layout_json`. One persistent engine, one loaded page, many queries.
Ships value on day one because the layout tree is already emitted.

**Phase 1 — the engine's own eyes** *(the real work)*
`hiwave_display_list` and `hiwave_style`. Both need new export paths in
`rustkit-renderer` / `rustkit-engine`. This is where the payoff is.

**Phase 2 — attribution** *(the payoff)*
`hiwave_diff(case, stage)` joining HiWave against the existing oracle and
naming the first divergent stage.

**Phase 3 — cross-platform**
Port to `hiwave-windows` (live, `parity-capture` already present) and
`hiwave-linux`. Can follow Phase 1 directly — see the correction in §4; this
is not gated on reviving anything.

---

## 6. What this is NOT

An MCP is an **exploration** surface, not a gate. Today `hiwave-macos` CI
never builds the workspace — every cargo invocation across all three workflows
is `cargo build --release -p parity-capture`, and there is no `cargo test`
anywhere; that is how `rustkit-svg` stayed uncompilable for 17 days and 40
green merges (#59).

An agent that can drive the browser interactively makes that gap *easier to
tolerate*, because problems get found by exploration instead of by CI. That is
the failure mode to name in advance: **this does not replace the workspace
build gate, and shipping it must not be a reason to keep deferring that
ruling.**

---

## 7. Open for Pete

1. ~~**Sequence.**~~ **ANSWERED by Pete, 2026-07-29: "do both."** Which is
   also where §9's lane analysis lands — Tier 3 (screenshots/compare) serves
   the porting seats now, Tier 1 (introspection) serves the pathfinder lane
   and the bucket-(b) unknowns. Neither waits on the other.
2. ~~**`hiwave_eval(js)`**~~ — **Atlas's call, taken: YES, behind
   `RUSTKIT_MCP_EVAL=1`.** It is the sharpest tool available and the fastest
   way to put the engine in a state no page can reach, which is exactly what
   a diagnosis lane needs. Off by default because an always-on script-eval
   surface in a browser engine is a foothold, not a feature — and a tool the
   fleet runs headlessly should not carry one silently.
3. ~~**Windows / Linux** — in scope or follow-on?~~ **ANSWERED by Pete,
   2026-07-28:** the platform topology in §8 is deliberate — a worker and a
   reviewer per architecture, macOS as pathfinder, ports downstream. So the
   MCP follows the same route every other feature does: build on macOS, port
   after. No separate decision needed.
4. ~~**Who builds it.**~~ **Atlas's call, taken: `/trench`, but not yet.**
   The Phase-0 scope in §9 is now demand-triggered and small enough that
   standing up a trench loop for it would cost more than the work. The trench
   earns its keep at Phase 1 — the engine export paths are repetitive,
   well-specified, and verifiable per-symbol, which is exactly the shape
   `/trench` is for. Until a porting seat reports the structural-mismatch
   trigger, this stays hand-sized.

---

## 8. Platform topology — and what it does to this design

Pete, 2026-07-28: *"this was my plan all along — have a worker on each arch
with a reviewer. Their repos were behind macOS but that's ok, because
progress on one may pave the way for the others."*

The lag is **designed**, not drift. Reading the roster against it:

| Arch | Worker | Reviewer | Repo |
|---|---|---|---|
| macOS | Atlas | Prometheus | `hiwave-macos` — pathfinder |
| Windows | Athena | Pollux | `hiwave-windows` |
| Linux | Talos | Argos | `hiwave-linux` |

macOS solves a chapter, the ports follow with the algorithm already proven.
That is exactly what W53 / W55 / W56, CSS Grid and radial gradients did.

Worth stating plainly because its absence caused a bad claim: this topology
is implied by the FLEET roster but written down nowhere. With no structural
prior saying "that repo has an owner and is active," a stale local clone was
enough to make me call a live tree dead (§4). Pinning it is the durable fix,
not resolving to read more carefully.

### The part that changes the tool surface

If macOS paves and the others port, then **the highest-value diff is not
always against Chrome — it is against macOS.**

The porting seats' hardest problem right now is receipt quality, and it is
their own reviewers saying so:

- *"Windows `rustkit-text` today can 'have tests' that never execute — reject
  that receipt shape."*
- *"Headless GPU capture is untrusted on BusyBee."*

So a porting seat currently proves a port with unit-test counts (which can be
silently cfg-gated out) or with pixel captures (which that machine cannot
trust). Neither answers the question the port actually asks: **did my port
compute the same thing macOS computes?**

A stage-wise introspection diff answers it directly, and it does so with
artefacts that are text, deterministic, and GPU-independent — layout trees
and display lists, not framebuffers.

**Design consequence:** `hiwave_diff(case, stage, reference)` where
`reference` is `chrome` **or** a committed macOS capture. Same machinery, one
extra argument, and it turns the MCP into the port-verification tool for two
of the three architectures rather than a debugging aid for one.

That also reorders the payoff: Phase 1's exports (display list, computed
style) stop being a macOS-only convenience and become the shared receipt
format the whole topology runs on. It is a stronger argument for
introspection-first than the one in §1, and it came from Pete's structure
rather than from the code.

---

## 9. REFUTED — the sequence is lane-gated, not absolute

Prometheus, answering as strategist so Athena can disagree with data rather
than re-derive the frame:

> **Default fleet answer for Windows right now: FIX-THROUGHPUT on known (a),
> DIAGNOSIS only when a class is still open.** … Screenshots-first wins on the
> (a) grind. Introspection-first wins only on open diagnosis classes. Atlas's
> MCP argument is not globally wrong; it is **phase-gated**.

**Accepted.** §1 argued introspection-first as if it were a property of the
tool. It is a property of the *queue*. I reached for the one example in front
of me — `about` at 16.17 with the residual guessed — and generalised from a
pathfinder diagnosis problem to a fleet whose dominant queue is porting.

His evidence is the stronger kind: the residual is now classified (§7), the
intrinsic_cache class is closed and merged, and what remains is 5,813 LOC of
portable port labour. That is throughput work. A deeper introspection tool
does not make it go faster; suite receipts and execute-counts do.

**One amendment.** "Phase-gated" reads as sequential — introspection later.
It is not sequential, because the lanes run **concurrently**:

| Lane | State | Wants |
|---|---|---|
| macOS pathfinder (Atlas) | diagnosis-bound — `about` 16.17 unattributed, bucket (b) unknowns open | introspection |
| Windows / Linux ports (Athena, Talos) | throughput-bound — classified (a) queue | screenshots + suite receipts |

So it is gated by **lane**, not by phase. Both are true today, of different
seats. The build order that follows is: Tier 3 (screenshots/compare) is what
the porting seats can use immediately; Tier 1 (introspection) serves the
pathfinder lane and the (b) unknowns. Neither waits on the other, and neither
is "the" answer.

### The first deliverable shrinks accordingly

Prometheus's Q2 answer is the discipline this plan was missing — **do not
build preemptively**:

> Conditional yes, narrow scope — not a standing second pipeline. … Ship first
> 2–3 (a) ports; if structural fails appear, then accept the text-diff tool as
> a surgical receipt — not preemptive scaffolding.

His minimum useful receipt, adopted verbatim as the Phase 0 scope, replacing
the larger Phase 0/1 in §5 as the *first thing built*:

- serialize the layout tree — node type, box, and a used-style subset
  (`display`, `position`, size, margin, padding)
- diff against a macOS dump of the **same fixture HTML**
- one command, local; CI optional and later

**Trigger, not schedule:** build it when a port lands green on Windows unit
tests but fails a shared suite case macOS passes, *and* the pixel delta is
uninformative. Not before. If Athena is mid-(a) with no structural failures
yet, the correct answer is "not now" and this section says so.

Explicit non-goals, from the same review: it must not become a required gate
before every (a) PR, and it must not need full display-list fidelity — that
would defeat the no-GPU property that makes it usable on hardware which cannot
be trusted to render.

### Standing caveat

Prometheus answered these two questions **for** Athena, as a default she can
override. She has not spoken yet. Her local read wins over the fleet template
on both — if her live feel is still "I don't know what's wrong," Q1's answer
is DIAGNOSIS and this section is wrong again.

---

## 10. Pete's rulings — all four answered (2026-07-29)

Recorded verbatim-in-substance, with what each one settles.

### 1. Introspection before screenshots — CONFIRMED

> *"Intro before screenshot."*

Settles the §1 reframe as written, and overrides the lane-gating hedge in §9
only in ORDER, not in substance: Tier 1 leads, Tier 3 still ships for the
porting seats. Prometheus's fix-throughput argument stands as the reason
screenshots are not dropped — it was never an argument for doing them first.

### 2. `hiwave_eval(js)` — BOTH DIRECTIONS

> *"both directions?"*

Read as: the eval surface should work in and out, not just push script in.
So the tool is **bidirectional** — evaluate an expression AND read what the
engine gives back, including thrown errors and console output, rather than a
fire-and-forget `eval` whose only signal is "did it crash."

That makes `hiwave_console()` part of the same tool surface rather than a
separate Tier 2 item: an eval whose result you cannot read is a write-only
debugger. Still behind `RUSTKIT_MCP_EVAL=1` — bidirectional does not mean
always-on, and an always-live script-eval surface in a browser engine is a
foothold.

### 3. Windows MCP — Athena's, only if it must differ

> *"let athena work on a windows mcp if it needs to be different, it shouldnt
> need to be once all baselines are functionally similar."*

This is the sharper version of what §8 said. The pin now reads: **there is one
MCP design, and a second implementation is a signal of a baseline gap, not a
platform requirement.** If Athena finds she needs a different tool surface,
that difference is a bug in baseline parity and gets reported as one — the
divergence is the finding, not the fix.

Practical consequence: Windows is not "port the MCP", it is "port the engine
exports the MCP needs." Same tool schema, same protocol, same smoke assertions.
A Windows-only tool name would mean the trees diverged where the topology says
they should not.

### 4. `/trench` — GO

> *"good trench candidate. Do it."*

Phase 1 goes to a trench loop. Scope, metric and stop condition are in §11.

---

## 11. Phase 1 trench — scope, metric, stop condition

A trench loop with no stop condition grinds forever and reports motion as
progress. Per the working rules, this one gets all three up front.

### The metric

**Engine export coverage: how many of the four Tier-1 reads the engine can
answer.** Today it is 1 of 4.

| Tier-1 tool | Engine support today | Needed |
|---|---|---|
| `hiwave_layout` | **YES** — `export_layout_json` (lib.rs:4608) | — |
| `hiwave_display_list` | no | new export on `rustkit-renderer` |
| `hiwave_style` | no | computed value **+ winning rule + origin** |
| `hiwave_diff(case, stage, reference)` | no | joins the two above against the oracle |

Metric is deliberately NOT lines of code or number of commits. It is
"can an agent ask this question and get an answer", which is checkable per
tool by a smoke assertion in `crates/hiwave-mcp/smoke.py`.

### Stop condition

**Stop at 4 of 4 with a smoke assertion per tool**, or on two consecutive
nights with no new tool answerable — whichever comes first. The second clause
matters more: an export that needs engine surgery will stall, and a loop that
cannot tell stalling from working is the decorative-gate failure in a different
costume.

### Per-night receipt

Not "worked on display list." Each night reports:

1. Which tool moved from no → yes, or explicitly **none**.
2. The smoke assertion that proves it, run and pasted.
3. What it cannot yet answer — the honest gap, named.

### Hard scope limits

- **No Windows/Linux work.** Per §10.3 the port is engine exports, not a second
  MCP, and that is not this loop's job.
- **`hiwave_diff` last.** It consumes the other three; building it first would
  mean stubbing what it consumes, which is the invent-a-baseline mistake.
- **No CI gate from this loop.** Prometheus's R1 stands: the MCP must not become
  the reason the workspace build/test gate keeps slipping.
- **Do not touch the parity harness.** It has had two honesty fixes in two days
  (#65, #69) and does not need a third hand in it this week.

### First night's target

`hiwave_display_list` — because paint is where every HiWave bug this year
actually lived (the ADVANCE CONTRACT, gradient axis routing, colour emoji, the
`rustkit-svg` break), and because §1's whole argument is that an agent should
be able to see the layout/paint boundary rather than infer it from pixels.
