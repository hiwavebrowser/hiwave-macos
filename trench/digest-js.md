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
