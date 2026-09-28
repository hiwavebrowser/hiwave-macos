# JS-ladder trench — PLAN

Ladder and rules: see atlas/trench-realsite trench/PLAN-realsite.md (entry "2026-09-27 22:45"). The design pin is trench/DESIGN-dom-bindings-rung0.md here (Prometheus, DESIGN CLEAR).

Order:
1. **Rung 0 read slice** (§5 of the pin): getElementById, querySelector/querySelectorAll (static NodeList), the textContent getter, and the documentElement/body/head getters, all backed by the Rust DOM. Build it with the §1 identity cache and the §2 NodeId-slot pattern. Ship it with the pins (a)-(d).
2. **§3 invalidation**: dirty bits → the existing restyle/relayout flush, coalesced at the end of the script.
3. **Mutation surface**: createElement, appendChild/removeChild/insertBefore, the textContent setter, setAttribute/getAttribute, classList, style. Mutation without the §3 flush is a land HOLD.
4. **Rung 1**: vendor the MDN learning-area (CC0) DOM examples into websuite/js-ladder/01-mdn/ in MDN's order. Score each against pinned Chrome 148 with parity-capture and track X/Y.

Rules:
- Branches are `atlas/js-<slug>` from origin/develop, each in its own worktree under ~/Repos/.worktrees/. Never touch the other lanes' worktrees or hubs.
- CARGO_TARGET_DIR=~/Repos/.worktrees/js-target
- Put the builtins campaign receipt in every PR body. Small commits that pass tests. Never force-push.
- macOS first (Pete, 2026-09-28): nothing waits on Windows or Linux.
