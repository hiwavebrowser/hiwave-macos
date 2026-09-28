# DOM bindings rung 0 — Prometheus design pin (exchange prometheus seq 764, 2026-09-27 ~22:50 ET)

Verbatim from the pin. **DESIGN CLEAR.** The read-only first slice (§5) may land now. The mutation surface may land once §3 (dirty-bit flush) is wired in the same or an earlier PR; no further design gate.

**Verdict: DESIGN CLEAR for rung 0.** blocks-land for the read-only first slice: **no**. blocks-land for any mutation surface: **yes** until §3 is wired.

### §1 Wrapper identity / cache
1. One live JsObject wrapper per Rust `NodeId` per document/view. Cache: `HashMap<NodeId, JsObject>` (or Boa equivalent) owned by the bindings host, not by the Node.
2. Identity must hold across entry points: `document.getElementById(x) === document.querySelector('#x) === el.childNodes[i]` for the same node.
3. Instance shape: thin host object with a private slot holding **NodeId only** (u64/newtype). Shared prototypes for Node / Element / HTMLElement (expand tag-specific prototypes later).
4. Detached nodes: wrapper stays alive while JS holds it; methods that need a tree may return null / throw NotFoundError; never UAF. On document navigate/destroy: drop the cache; lingering wrappers fail soft on NodeId lookup.
5. Do **not** store `Rc<Node>` / raw pointers inside Boa-managed objects.

### §2 GC rooting across Rust ↔ Boa
1. Rust owns the Document/Node arena. Wrappers are GC-managed JsObjects whose only host payload is NodeId. Boa GC does not need to trace into Rust heaps.
2. Long-lived JS roots that already exist: `window`, `document`, and (once wired) `document.documentElement` / `body` / `head`. Those keep the corresponding wrappers alive via ordinary JS reachability.
3. Rust must **not** hold strong JsObject refs across script turns except for those globals / an explicit host root table. Prefer recreating from cache by NodeId.
4. If a Node is destroyed in Rust while JS still holds a wrapper: NodeId lookup misses → null / throw; never dereference freed Rust memory.
5. Cache eviction: drop entries when the document dies; optional weak/finalizer prune later — not required for rung 0 reads.
6. Non-goal this rung: exposing Rust `Rc` into `boa_gc::Trace` graphs.

### §3 Mutation → style/layout invalidation (GATES the mutation surface)
1. **Read-only first slice needs no invalidation** (getElementById / querySelector[All] / textContent getter / documentElement|body|head).
2. Before any of createElement+append*, remove*, textContent/innerHTML **setters**, setAttribute for style-affecting attrs, or className/classList mutations land: wire dirty bits into the existing Engine restyle/relayout path (same flush used after load_url script settle).
3. Buckets:
   - structure insert/remove/move → mark subtree (+ ancestors as required) style+layout dirty;
   - text content change → text/layout dirty;
   - style-affecting attrs (`style`, `class`, `id` when selectors depend on it) → restyle;
   - non-style attrs → no restyle.
4. Coalesce: set dirty during script; flush once at end of script / microtask checkpoint / settle. Sync forced layout only when a later rung adds getComputedStyle / getBoundingClientRect.
5. Any PR that mutates the live Rust DOM without this flush = land HOLD from this seat.

### §4 Out of scope (rung 0 / first slices)
Custom elements + Shadow DOM (rung 5); MutationObserver / IntersectionObserver / ResizeObserver; full JS addEventListener → Rust dispatch wiring (events.rs stays Rust-side for now); live HTMLCollection (static NodeList OK for querySelectorAll); iframe / browsing contexts; CSSOM mutation; replacing Boa; privacy/tracker policy (separate pin from atlas #520 — still owed, not this note).

### §5 First slice — GO without further design gate
Allowed now on `atlas/rs-*` from origin/develop:
- `document.getElementById`
- `document.querySelector` (+ `querySelectorAll` returning a **static** NodeList)
- `textContent` **getter**
- `document.documentElement` / `body` / `head` getters
Must ship with §1 identity cache + §2 NodeId-slot pattern even for reads (no second stub document). Pins: (a) getElementById returns same object as querySelector('#id'); (b) body textContent matches Rust DOM text; (c) missing id → null; (d) after navigate, old wrappers do not read the new document's nodes.

Mutation surface (createElement/append/set textContent/innerHTML/classList) waits on §3.

