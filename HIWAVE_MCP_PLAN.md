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
Port to `hiwave-windows` once that repo is awake. Not before.

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

1. **Sequence.** I want introspection first and screenshots second, for the
   reasons in §1. That inverts the natural reading of "test and display." Say
   if you want it the other way — it is your call and the work splits cleanly
   either way.
2. **`hiwave_eval(js)`** — yes or no? It is the sharpest tool and the easiest
   way to put the engine in a state no page could reach.
3. **Windows.** Confirm it is out of scope for the first cut, or fund the
   repo revival as part of it.
4. **Who builds it.** This is a well-specified, mostly-mechanical crate over
   an existing engine — a good `/trench` candidate rather than hand-driven
   work, once Phase 1's export paths are designed.
