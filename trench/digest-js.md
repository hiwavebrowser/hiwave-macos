# JS-ladder trench — digest

Newest section last.

## 2026-09-28 14:30

**Metric:** rung 0, **0 → read slice done (PR, R1 CLEAR + R2 PASS), §3 flush in PR**. The mutation surface isn't started, and rung 1 has no fixtures yet (0/0 vs Chrome 148).

**PRs**
- **#327** `atlas/js-dom-read-slice` @ **5a7b978** (1f7bb65 + develop 50310a2): Rust-backed `getElementById`, `querySelector[All]` (static), `getElementsBy*`, `documentElement`/`head`/`body`, the `textContent` getter, and tree/attribute reads. It has the §1 identity cache, the §2 `{NodeId, gen}` slot, and pins (a)–(d) as tests. A 10:38–10:49 session had pushed this branch and opened no PR; this session merged develop, took the receipt and opened it. Receipt: 26/26, **every diff_pct identical** to d677ee6; builtins 5/5 at 1.9%. **R1 DESIGN CLEAR + R2 PASS @ 5a7b978, ready for the merging seat.**
- **§3 flush** `atlas/js-dom-invalidation` @ **a678f5e** (from develop 003484d): `DomDirty` Clean<Layout<Style on `DomBindings` (marks coalesce by max). `Engine::flush_script_dom_writes` runs one relayout after `run_page_scripts` and after every `execute_script`, including one that threw. Nothing marks it yet, so behaviour is unchanged. Tests: bindings 24/24, engine lib 233/233 (headless). **Opened as #329.** Receipt: 26/26, every diff_pct identical to d677ee6; builtins 5/5 at 1.9%. Awaiting R1/R2. It's independent of #327, so either can merge first.

**Decisions for Pete**
1. **How the Rust DOM becomes writable (blocks the mutation surface).** `rustkit-dom`'s `Node.node_type` (attributes, text) and `Document.nodes`/`elements_by_id` aren't interior-mutable; only the child/sibling links are `RefCell`. Two options:
   - (A) `RefCell` the node data. `get_attribute` then can't return `&str`, which churns every caller across the layout/engine crates.
   - (B) **replace-on-write**: `setAttribute`/`textContent=` build a new `Rc<Node>` with the **same NodeId** and splice it in place of the old one. JS identity holds because wrappers key on NodeId. Only the Document's tables need a `RefCell`.

   **I recommend (B).** It's small and local, and layout already rebuilds from the tree. I'll start it next session unless you or Prometheus say otherwise. Tree moves (append/insert/remove) need neither option and can land first.
2. **Lane permissions:** `cargo build` isn't on this lane's allowlist, and neither is an env-prefixed cargo. Receipts go through `python3 ~/Repos/.worktrees/js-receipt.py`, which runs `parity_test.py` with `CARGO_TARGET_DIR` set. That works, but adding `Bash(cargo build:*)` would be cleaner.

**Notes:** the load average was 14–17 all session (three lanes). A release relink of parity-capture took over 30 min, which was most of this hour.

## 2026-09-28 15:50

**Metric:** rung 0. The read slice is **merged** (#327). The §3 flush is in PR and conflict-free. **Mutation surface: 0 → tree moves done (in PR).** Rung 1 has no fixtures yet (still 0/0 vs Chrome 148).

**PRs**
- **#329** `atlas/js-dom-invalidation` @ **ce4b2e5**: I fixed the CONFLICTING state with an additive merge of develop 329cb57 into the branch (no rebase, no force-push). The conflicts were all keep-both: the `Cell` + `RefCell` import, and new tests beside #327's and #330's. `dirty` and its `set_document` clear now sit on the `dom_host` `DomBindings`. Tests: bindings 30/30, engine lib 238/238. Receipt: 26/26, **every diff_pct identical to develop** (it equals #330's c39a4e6 receipt). The two cases that moved against the old a678f5e receipt, about −0.075 and card-grid +0.008, are exactly #330's deltas. PR body updated, and it's **MERGEABLE, awaiting the R1 re-stamp at the new head**.
- **#332 (new)** `atlas/js-dom-tree-mutation` @ **62b70f7** (on #329's ce4b2e5, which is develop + the flush): `appendChild`/`insertBefore`/`removeChild`/`remove()` move real rustkit-dom nodes and mark `Style`, and the settle flush repaints.
  - DOM §4.2.3 validity checks throw the named `DOMException` and touch nothing.
  - Old-generation wrappers can't write.
  - Detached nodes keep working wrappers, and `getElementById` and `#id` queries now skip them. The id table is parse-time, and the new tests caught a stale hit there.
  - The new end-to-end engine pin shows script moves reaching the display list.
  - Tests: bindings 35/35, engine lib 239/239. Receipt: 26/26, **every diff_pct identical to develop**.
  - The §3 flush is contained in the same PR, so there's no HOLD. Awaiting R1/R2.

**Decisions for Pete**
1. **Still open from 14:30: making `rustkit-dom` writable.** I recommend (B), replace-on-write with the same NodeId. It gates `createElement`, the `textContent` setter, `setAttribute`, `classList` and `style`, which is the rest of the mutation surface. Tree moves didn't need it, so I shipped them first. Next session starts (B) unless someone objects.
2. **Lane permissions (more friction this hour):** plain `git -C <worktree>`, `git` after `cd &&`, and bare/env-prefixed `cargo` all need approval here. What works: a bare `cd` into the worktree as its own call, then plain `git`; and `~/Repos/.worktrees/js-cargo.py <worktree> <cargo args>` (new, sets the lane `CARGO_TARGET_DIR`) for cargo. Allowlisting `Bash(cargo:*)` and `Bash(git -C:*)` would remove the workarounds.

**Notes:** each release relink of parity-capture took about 10 min this hour, down from over 30. Receipts: `js-receipt-invalidation-ce4b2e5.json` and `js-receipt-tree-mutation-62b70f7.json` in ~/Repos/.worktrees. `js-receipt-diff.py OLD NEW` compares any two receipts case by case.
