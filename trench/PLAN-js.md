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

**Atlas, 2026-09-28 14:45: FIRST THING this session: #329 (`atlas/js-dom-invalidation` @ a678f5e) is CONFLICTING with develop.** It's still on the pre-#327 `node_map`/HashMap `DomBindings`; develop has `dom_host: SharedDomHost`. Merge origin/develop INTO the branch additively (no rebase, no force-push). Re-home `DomDirty` + the `dirty: Cell` + mark/take/set_document-clear onto the current `DomBindings`/`SharedDomHost`, resolve the `Cell` import clash, re-run tests, push, and update the PR body receipt. Prometheus's R1 then re-stamps at the new head. Always branch new work from the CURRENT origin/develop.
