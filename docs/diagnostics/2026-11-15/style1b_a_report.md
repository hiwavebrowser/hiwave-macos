STYLE1b (a) report (Athena). Branch athena/style1b-measure @ ca3ae8f8, develop be8eb272. Measurements only; Windows, parity profile.

1. HOST-CALL FLOOR AND PER-STAGE COST (5000 tiles, 15000 ops each; style1b_cost_tests, --ignored)
| stage | us/op | parses/op |
| empty host call (floor) | 0.25 | 0 |
| d.nodeType (one wrapped host read) | 1.3 | 0 |
| getAttribute('style') | 1.3 | 0 |
| setAttribute('style', text) changing | 11.1 | 0 |
| d.style (Proxy fetch) | 2.9 | 0 |
| style.left read, cache hit (one tile) | 9.2 | 0 |
| style.left read, across 5000 tiles | 63 | 0.30 |
| style.left = same (no-op) | 40 | 0 |
| style.left = v, changing | 74 | 0 |
| Multiply mix (3 props per tile) | 65 | 0 |
| changing write, MutationObserver installed | 112 | 0 |
| plain JS WeakMap.get / Map.get / Proxy trap | 0.3 / 0.35 / 0.45 | |
- One JS parse of a three-declaration attribute costs ~180 us in Boa (reads: (63 - 9) / 0.30).
- A changing write (74 us), split with a temporary uncommitted timing patch and checked against the uninstrumented total: exactOf ~17.5, hasBadUrl ~16.5, decls (list copy) ~12, setAttribute ~11, declNamed ~6, serialize ~4.4, keep ~3.7, normalise ~2.7. So ~34 us of 74 is #1170's hardening checks (char-code loops in Boa), ~18 is reading/copying the kept list, ~11 is the attribute write. Allocation is additive (no collector cliff), and the 4096 log cap does not slow writes.
- The Rust side of a write is small: setAttribute 11 us includes the wrappers, two script-hook host reads (~2.6) and the AttrMap clone + new Node + log.

2. A MULTIPLY FRAME THROUGH THE ENGINE (style1b_frame_cost_tests, --ignored; 5000 abs-pos tiles, 6 inline decls each, 3 written per tile)
| frame | execute_script | page's writes | flush (restyle+layout) | next live turn | render |
| 0 | 3124 ms | 3058 | 66 | 0.5 | 9.9 |
| 1 | 1827 ms | 1753 | 74 | 0.4 | 9.6 |
| 2 | 2056 ms | 1990 | 66 | 0.4 | 9.5 |
96-98% of the frame is the JS writes. The flush is 66-74 ms even past the 4096-record cap (whole-page restyle). Writes cost more here (117-200 us) than in the micro: six declarations per tile lengthen every char-loop, and frame 0 parses every tile once.
Consequence for the order: (b)+(c) are the win; (d) is worth at most ~70 ms/frame; (e) only matters if the flush grows once writes are fast. Decide (d)/(e) on numbers after (c).
Mac Multiply (the headline you asked for) not run: this box cannot. For the batch: `python3 tools/stress/run.py --suite motionmark --only Multiply --test-interval 10 …` on develop, then after (b) and (c).

3. CHROME CONTRACT (style1b_contract_page.js, CfT 148.0.7778.216, 3 runs identical; 25 cases)
RustKit differs today in 14 (style1b_contract_tests ledger, KNOWN_GAPS):
attr-invalid-dropped, attr-verbatim (length 5 vs 2: margin not expanded), color-normalised, csstext-messy, custom-property (--Y lowercased), remove-attribute-then-style (null vs ""), set-invalid-ignored, set-unitless-ignored, shorthand-background (length 9 vs 1), shorthand-in-attr, shorthand-margin, shorthand-margin-then-longhand, unknown-property-ignored, url-and-quotes ('Open Sans' vs "Open Sans").
Root: inline style has no value parser at all; strings in, strings out. Matches Chrome: verbatim attribute text, cssText of clean text, order kept on update, no-op write does not rewrite or record, priorities, removeProperty, transform, camel/float, the MutationObserver record sequence (3 records with their oldValues; the repeated same write makes none).
Consequence for (c): the Rust block should hold PARSED declarations (rustkit-css's property parser on write: validation, normalised serialization, shorthand expansion), not strings; a string Vec would bake these 14 gaps in. That also makes the write path Chrome's shape (parse value, compare, store).

4. WEB ANIMATIONS LAYER
Animated values are NOT written through the style attribute (only commitStyles(), by spec). They live in AnimatedStyles.script (Rust HashMap by node id, dom.rs:142-158), applied after the full cascade (lib.rs:13316 → layout_ctx.rs apply_script_layer). Invisible to getAttribute/cssText/length/MutationObserver; visible to getComputedStyle. The engine record is MutationKind::Style with attr None (layer-only paint path). So no order change; (c) must keep InlineStyle and this layer separate (it already is).
Found on the way (rows, not STYLE1b): (i) the script layer wins over inline !important (no importance check in apply_script_layer; in CSS, important author/inline beats animations) - unverified against Chrome; (ii) stale comment rustkit-bindings/src/lib.rs:1335 says animated values go into inline style.

5. HOLDERS OF OLD NODE DATA (for (b) in-place writes)
Node data is Rc<Node> with a plain node_type (no RefCell); replace_node_data builds a new Rc with the same NodeId and relinks. One Rc<Document>, no layout/paint snapshot.
- BLOCKER: svg_prepass caches (svg_prepass.rs:149-154 Entry::made_from, 233-240 element_paint) treat "same Rc pointer" as "same data" and survive a generation change while the node lives. With in-place writes a changed svg attribute or ancestor class would reuse a stale key/paint. Needs a per-node data version (or keying on it) in (b).
- elements_by_id: in-place `id` writes must keep its retain/re-insert logic.
- RefCell on attributes: every &node.node_type match changes; a borrow held across a script callback would panic instead of reading stale. Needs an audit in (b).
- Benefits: the parser's open-elements stack (lib.rs:562-565 defers attribute merges because of replacement), Rust event dispatch holding an Rc.
- Safe (keyed by NodeId or per-build): StyleMemo, selector memos, AnimatedStyles, focus/hover, restyle invalidation, MutationLog (ids + old value strings), JS wrappers (gen, NodeId), templates, cloneNode (deep copy), a11y, devtools/inspector/mcp.
- Tests asserting replacement semantics to rewrite: rustkit-dom lib.rs:1680-1694, 1785-1786, shadow.rs:613.
Alternative for (b) that avoids the RefCell audit: keep replace-on-write for every attribute except a style write that only touches the InlineStyle (itself behind its own RefCell), and bump a node data version that svg_prepass checks. I lean to this; your call.

PROPOSED NEXT: (b') = InlineStyle cell + data version + svg_prepass keyed on it, attribute writes otherwise unchanged; then (c) with parsed declarations, closing contract gaps as they fall out, the ledger shrinking.
