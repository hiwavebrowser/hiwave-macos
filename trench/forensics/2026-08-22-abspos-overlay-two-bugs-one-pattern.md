# n29 (2026-08-22): the WPT overlay pattern was two engine bugs, neither of them text

**PR #152** (`atlas/abspos-overlay`, off master). Boards: Tier-1 7/25 → 9/25 on branch; campaign 26/26 avg 6.6 → 6.5, zero regressions.

## Bug 1 — layout: abspos sibling swallows the pending block margin

`layout_block_children_with_collapse` passed the parent's live `MarginCollapseContext` into out-of-flow children. The abspos child resolved the pending margin into its own static position, then `reset()` the context — the next in-flow sibling lost the inter-block margin entirely. Receipt: lba001 green cover (abspos) y=56, red text div y=40 — exactly the `<p>` 16px bottom margin. Fix: out-of-flow children resolve against a clone (CSS 2.1 §8.3.1: out-of-flow boxes do not participate in margin collapsing). Unit test asserts 46.0/46.0 (was 30.0 for the in-flow sibling).

## Bug 2 — renderer: solid fills could never paint over text

`flush_batches_to`/`flush_to` draw ALL color quads, then ALL glyph quads. Any solid fill AFTER text in the display list (positioned boxes paint after in-flow content per CSS 2.1 App. E — the display-list builder gets this right) was silently drawn UNDER that text. Receipt: red-green blend pixels `(105,75,0)`-class over the green cover where ref is pure `(0,128,0)`. Fix: fast path flushes batches before a `SolidColor`/`RoundedRect` whose transformed rect overlaps a batched glyph quad — the discipline `execute_with_gpu_gradients` already applied to gradients. Common pages (backgrounds before text) take zero extra flushes.

## The attribution ledger over-claims

The Tier-1 runner attributes 12–14 fails to "@font-face unimplemented". Tonight's two flips were in that family's neighborhood and needed no fonts. The runner itself now marks 3–5 PASSes as SUSPECT (web font never loaded). Treat `blocked_by: @font-face` as "declares a web font", not "fails because of it".

## HANDOFF — #150 interaction, measured (stack probe `atlas/n29-stack-probe`, local only)

Stacked tree (slice-0 + this PR) = **9/25, not 11/25**: slice-0's real `line-break:anywhere` re-fails lba001/002 at 0.085%/0.068%. Layout boxes are CORRECT on the stack (12+6 lines, each 1ch wide, y=56 aligned with the cover) — but glyph INK paints ~2–3px right of the box on several lines (diff columns x17–19 vs cover right edge 16.9, rows = lines 1–3+ of the column). Master's accidental one-grapheme-per-line fallback placed ink correctly; slice-0's breaker path shifts it. Suspect: paint-side advance/x-offset under the new per-unit line production. One-night fix in the #150 lane; whichever PR lands second inherits the ~0.08% residual until then. Receipts: `scratch_n29/stacked-board-receipt.json`, `scratch_n29/lba001.{test,ref}.ppm` + diff row dump in the n29 digest session.

## Named residuals (ledgered, not chased)

- `empty-span-size-002` 0.011% (52 px): empty-inline line-box height semantics (css-inline-3 invisible-line-boxes), ruby with border materializes a 16px line where ref collapses to 0. Small, self-contained.
- Blur path (`execute_with_gpu_blur`) keeps the old two-batch order — same occlusion hazard, unmeasured, no Tier-1 case hits it.
- Overlap check vs transforms: batched glyph positions are final; incoming rect corners are transformed; nested-transform false positives cost one extra flush, never correctness.
