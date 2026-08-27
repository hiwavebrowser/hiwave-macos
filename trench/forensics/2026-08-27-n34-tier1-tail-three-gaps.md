# n34 — the Tier-1 tail was three engine gaps, none of them text shaping
2026-08-27, macOS trench seat (Atlas). Branch `atlas/n34-wpt-tail` → develop.

## Board
| | develop `d223b31` (fresh build, reproduced) | `atlas/n34-wpt-tail` |
|---|---|---|
| WPT Tier-1 scored | 18/26 (69.2%), 0 ERROR | **21/26 (80.8%), 0 ERROR** |
| campaign pixel board | 26/26, avg 4.1% | **26/26, avg 4.1%** (zero movement, zero regressions) |

Every number on a fresh-built `parity-capture` with the n24 freshness guard on.

## Method
Capture test+ref for each fail, diff bbox, coarse colour map (`scratch_n34/probe.py`,
`map.py`). The maps localised each fail to a box before any code was read — and in
all three Ahem cases the `.test` text was ALREADY laid out correctly; the diff lived
in the fixture's red overlay or its green cover.

## The three gaps
1. **overflow-wrap-anywhere-005 (2.08% → 0).** The `.fail` overlay rendered as two
   300px rows: `<span>XX<br></span>` — a `<br>` INSIDE an inline is a 0×0 nothing
   because both inline-flow loops only close a line for their DIRECT `LineBreak`
   children (n33 ledgered this). rustkit-engine now splits the inline around each
   break at push time, CSS 2.1 §9.2.1.1 continuation style: `<span>a<br>b</span>` →
   `[span(a)] [br] [span(b)]`, identity kept on the first piece only, nested inlines
   split level by level because every parent runs the same push.
2. **overflow-wrap-anywhere-001 (0.96% → 0).** Text wrapped right; the
   `::after { position:absolute; inset:0 }` green cover was 54px tall on a
   `height:100px` div. Both inline-flow loops lay out abspos children with
   `cb.content.height = cursor_y` — that is the Robinson static-position trick
   (`calculate_block_position` stacks at `cb.y + cb.height`), NOT the containing
   block's height, so `bottom`/`inset` resolved against "content laid out so far".
   Fix: `definite_content_height()` (absolute `height` only; percentages need the
   grandparent) and `reanchor_absolute()` after the child lays out — re-resolves
   the offsets against the real block and carries the subtree. `position: fixed`
   is untouched (viewport CB). Real-page shape: every `inset:0` cover/overlay on a
   fixed-height container whose content is shorter than the container.
3. **line-break-anywhere-005 (0.39% → 0).** Lines came out `X XX` / `XX X` /
   `XX X`: the wrapper skipped collapsible white space at every soft break
   regardless of `white-space`. Under `break-spaces` (css-text-3 §4.1.1) a preserved
   space at a break is content on the NEXT line. `white-space` is now plumbed into
   the wrapper (`wrap_text_white_space`, `wrap_text_mid_line_white_space`); the
   legacy entries keep collapsing. `pre-wrap` deliberately keeps the skip — its
   trailing spaces hang (§4.1.3), which dropping them already approximates.

## Ledgered, not chased
- **overflow-wrap-anywhere-002/003 (0.05/0.08%) are a MONOSPACE family, not
  break-spaces.** Fine maps: owa003 renders `PAS` / `S` / `FAIL` — `4ch` resolves
  narrower than four monospace glyphs (the `ch` advance is not taken from the
  element's resolved monospace face), AND `overflow: hidden` with `height: 2em`
  does not clip the third line. owa002 renders `FAIL` on line 1 with no leading
  space: `<div> FAIL <div>` reaches layout without its edge spaces despite
  `break-spaces`, so `" FAIL"` fits `5ch`. Three separate terms; one night.
- lba001/002 (0.0196/0.0131%): unchanged — one column of ~1% AA coverage; the
  tolerance decision from n33 still stands with Pete.
- empty-span-size-002 (0.0102%): outline paint (n21).
- `reanchor_absolute` does not fix a percentage `height` on the abspos child itself
  (calculate_block_height still sees the stand-in height on the first pass).
- The campaign board did not move for any of the three: no campaign page has a
  `<br>` inside an inline, an `inset:0` cover shorter than its container, or
  `break-spaces`.

## Receipts
Commits on `atlas/n34-wpt-tail` (see PR). Tests: `abspos_inset_fills_parents_
definite_height_not_its_flow_cursor`, `abspos_reanchor_leaves_auto_height_parents_
alone`, `test_wrap_break_spaces_keeps_the_space_at_a_soft_break` (rustkit-layout),
`br_inside_an_inline_is_hoisted_to_the_block_flow` (rustkit-engine, GPU-gated like
its neighbours). Scratch: `scratch_n34/` (probe + map scripts, captured frames).
