# HiWave MCP Phase 0 — outside-eye design review (PR #66)

**Date:** 2026-07-29  
**Seat:** Prometheus (design / outside-eye only)  
**PR:** [hiwavebrowser/hiwave-macos#66](https://github.com/hiwavebrowser/hiwave-macos/pull/66)  
**Branch:** `atlas/hiwave-mcp-phase0` @ `d720840`  
**Plan:** `HIWAVE_MCP_PLAN.md` (PR #61) + prior Prometheus pins `33d48e8e48cb`, `f080a273c889`  
**Verdict:** **APPROVE** — Phase 0 scope honest; ship as pathfinder tooling. Merge is Atlas/Pete lane.  
**This seat does not merge, force-push, or open follow-up PRs.**

---

## 1. What was measured

| Check | Result |
|-------|--------|
| File set | `crates/hiwave-mcp/{Cargo.toml,src/main.rs,smoke.py}` + workspace member in root `Cargo.toml` / lock |
| Size | +444 / −0; ~270 LOC Rust server |
| Architecture | In-workspace binary; `rustkit-engine` + `headless`; no new SDK |
| Tools | `hiwave_open`, `hiwave_layout`, `hiwave_screenshot`, `hiwave_status` |
| Local build | `cargo build -p hiwave-mcp` **OK** (worktree @ d720840) |
| Local smoke | `python3 crates/hiwave-mcp/smoke.py` **PASS** (layout thesis 432×152) |
| GitHub checks | pr-swarm 0–3 pass; audit pass (as of review) |
| CI compiles this crate? | **No** — all workflows still `cargo build -p parity-capture` only |

---

## 2. Verdict vs prior design pins

| Prior pin | Phase 0 code check |
|-----------|-------------------|
| In-workspace, not new repo | **PASS** — `crates/hiwave-mcp` |
| In-process engine (parity-capture pattern) | **PASS** — direct `EngineBuilder` / headless view |
| Phase 0 = wrap existing export/capture, persistent process | **PASS** — open once, many queries |
| Screenshots in Phase 0 OK; product value ranks introspection | **PASS** — layout is first-class; screenshot present, not the thesis |
| MCP does **not** replace workspace CI gate (HARD NO) | **HOLD residual** — still true and still open; see §4 |
| Windows/Linux port of MCP not in first cut | **PASS** — macOS pathfinder only |
| Port-receipt spine `reference ∈ {chrome,macos}` | **N/A Phase 0** — correctly deferred (Phase 1+ / demand-trigger) |
| `hiwave_eval` only behind `RUSTKIT_MCP_EVAL=1` | **PASS** — no eval surface; JS disabled on engine |
| Plan §9 demand-trigger (macos-ref text dump for ports) | **Not a blocker for this PR** — that pin gates *port receipt scaffolding*, not the pathfinder persistent server |

**One-line product check:** smoke asserts content-box `.hero` border box = 432×152 from the layout tree. That is the thesis (“pixels can lie about stage; layout cannot”). Correct for Phase 0.

---

## 3. Design quality (approve reasons)

1. **Scope honesty.** No display-list, no cascade, no oracle join, no eval. PR body names Phase 1/2 residual explicitly.
2. **Protocol hygiene.** NDJSON JSON-RPC on stdio; diagnostics on stderr; `notifications/initialized` unanswered; tool failures as `isError` in *result* (MCP agent-visible), not silent crash.
3. **Ambiguity refusal.** `html` XOR `path` — typo cannot masquerade as a render bug.
4. **Persistence property tested.** Status after open proves session survival (the delta vs parity-capture).
5. **Parity-test engine config.** `EngineConfig::for_parity_testing()` + UA `HiWaveMCP/0.1` + JS off — correct defaults for diagnosis, not a silent product browser.
6. **No dependency bloat.** Hand-rolled MCP handshake instead of an SDK is the right call for this workspace’s CI/dep discipline.

---

## 4. Residuals (non-blocking unless marked)

| # | Residual | Severity | Owner |
|---|----------|----------|-------|
| R1 | **CI never builds `hiwave-mcp`.** Same class as #59 (workspace not built). Plan §6 already named this failure mode. Shipping Phase 0 must not be used to defer a workspace / multi-package gate. | **Design residual · recommend follow-up** | Atlas (gate PR) / Pete if scope debate |
| R2 | **Smoke not wired into CI.** Local PASS is a receipt; it will rot if not run somewhere. Prefer a cheap job: `cargo build -p hiwave-mcp && python3 crates/hiwave-mcp/smoke.py` on macOS. | Medium | Atlas |
| R3 | **Unused `rustkit-layout` dep** + `_unused` dead fn — copy-paste from parity-capture; drop in follow-up or same PR nits. | Nit | Atlas |
| R4 | **No selector filter on layout.** Whole tree only. Fine for Phase 0; large pages will force agent-side search. Phase 0.5 optional: `selector?` if engine already can filter. | Optional | later |
| R5 | **Screenshot = PPM**, not PNG (plan Tier 3 said PNG). Acceptable Phase 0 (`capture_frame` already ships PPM); document for agent clients. | Nit / docs | Atlas |
| R6 | **Path open is full FS read** for the agent process. Correct trust model for *local seat tooling*; not a multi-tenant server. Document one line in README/tool description. | Docs | Atlas |
| R7 | **Results re-serialized as pretty JSON text** inside MCP content. Smoke re-parses; works. Structured content later is polish, not gate. | Nit | later |
| R8 | **`hiwave_diff(..., reference=macos\|chrome)`** and display-list / style exports remain the high-value Phase 1 spine. Do not expand #66 to absorb them. | Standing pin | pathfinder after merge |

**None of R1–R8 is a design REJECT.** R1 is the only strategic residual that should be tracked on the board after merge.

---

## 5. Explicit non-claims

- This is **not** merge authorization from Prometheus (Atlas merges on green + Pete habit).
- This is **not** clearance to skip workspace CI work.
- This is **not** Windows/Linux MCP port GO (topology: macOS pathfinder first).
- This is **not** Phase 1 export design (display-list / computed-style) — still open design when Atlas opens that PR.
- Demand-trigger for **port text-diff receipts** still stands for Athena/Talos; it does not veto macOS Phase 0 server.

---

## 6. Seat asks

| Seat | Ask |
|------|-----|
| **Atlas** | Merge #66 when ready (own lane). Optionally fold R3 nits. Open or bank a follow-up for R1+R2 (build+smoke gate). Do not expand scope mid-PR. |
| **Pollux** | No execute-count gate on this PR (macOS tooling, not Windows port). Optional: note smoke PASS shape if reviewing. |
| **Athena / Talos** | No action on #66. Continue rails / ports. When a structural port fail needs text receipt, *then* escalate macos-ref dump / `hiwave_diff` — do not pre-scaffold. |
| **Pete** | None required for design. Optional: prioritize R1 workspace/MCP compile gate after merge if CI debt still bites. |

---

## 7. Prometheus next

Outside-eye **first new open PR** needing design after this tick. Do not re-review #66 unless measurement changes (new commits that expand scope, wire eval, or claim CI gate replacement).
