## n43: a parent's open edges never collapsed margins with its children; form controls dropped their author margins

**Lane:** css-selectors 11.20 — the n42 digest's option (b), taken from develop tip `5b89ed8` (#174/#173 still open at lane start; nothing stacked).

### What was wrong (rustkit-layout `lib.rs`)
1. **CSS 2.1 §8.3.1 parent/child through-collapse was not performed** — `layout_block_with_collapse` gave every child list a fresh margin context and said so in a comment. `.section-title{margin-bottom:10px}` + unpadded wrapper + first child `margin-top:4px` laid the child at 10 + 4 (Chrome: max = 10); a wrapper-in-wrapper doubled it; the last child's bottom margin through an open bottom edge was dropped. `should_collapse_with_first_child` existed with zero callers.
2. **`layout_form_control` never resolved `margin-*`** — `button{margin:4px}` / `input{margin:4px 0}` rows measured the bare rect (33.5 / 37.5 vs Chrome 39 / 43).
3. **`baseline_is_bottom_edge` kept the bottom-edge model for author-padded controls** — calibrated while (2) was true; with margins it overshoots (41.5). Text-line controls now use the hang model; checkbox/radio use the bottom edge (Blink), which builds the 23px checkbox+label line.

### The fix
- `MarginCollapseContext` gains `children_are_formatting_roots` (flex/grid containers set it; the engine sets it for the root element), `first_child_top_adjoined`, `last_child_collapses_through`.
- Top edge: `first_child_top_margin_chain()` walks open first in-flow block children and the box adjoins its own top margin + the chain before positioning; the first child skips its own margin.
- Bottom edge: with `should_collapse_with_last_child`, the last child's margin stays pending and the parent `absorb()`s it next to its own bottom margin. Closed edges and formatting roots keep it inside (flex/grid item re-layouts used to drop it).
- Engine: root context flag (one line) — body + h1 collapse under html's edge, html stays at 0.
- Form controls resolve their four margins into the margin box.

### Receipts
- **Tests:** rustkit-layout 308 → 315 (first-child through an open parent; chain through two wrappers; last-child adjoins the next sibling and does not inflate the parent; padded parent keeps the margin inside; flex item keeps both edge margins inside; root element does not collapse with body; form control carries its author margins). rustkit-engine 69/69. T-RED: the assertions carry the old numbers (34 = summed, 32 = dropped).
- **Repro** `parity-tests/repro/margin-through-collapse.html` vs pinned Chrome 148: sections A–D (chain, padded negative control, flex item, ul) match to 0.0px on all 25 elements; E (controls) within 0.7px except the label text next to the checkbox (−6.4, ledgered).
- **Campaign board** (basis: n39 clean-develop capture on this same sha, avg 4.0404) → **avg 3.6746**: css-selectors 11.2043 → 2.6694 (−8.53pp), combinators 2.4780 → 1.9466 (−0.53), form-elements 3.8742 → 3.4294 (−0.44), new_tab −0.002, form-controls +0.001; 21 of 26 byte-flat. css-selectors geometry mismatches 43 → 1.
- **WPT Tier-1:** 24/26 flat, same two fails, last-run pinned on the engine commit.

### Ledger
- Label text beside a bottom-edge checkbox does not drop to the checkbox baseline (Slice C `apply_vertical_align`) — the one remaining css-selectors geometry term.
- Percent margins in the through-chain resolve against the top box's content width.
- Out-of-flow children are still laid out against the parent's live margin context (pre-existing).
- Control heights 30.33/34.33 vs Chrome 31/35 (composed-height rounding).

Forensics: `trench/forensics/2026-09-05-n43-margin-through-collapse-and-form-control-margins.md` (hub). Overlap check: `lib.rs` block-flow + form-control sites; #176 touches `lib.rs` (atomic inline width) and #180 (abspos shrink-to-fit) — different functions, restack to confirm.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
