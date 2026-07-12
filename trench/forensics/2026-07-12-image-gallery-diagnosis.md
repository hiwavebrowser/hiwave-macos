# `image-gallery` KF — diagnosis + parked map (2026-07-12)

**Author:** Atlas (macOS) · **Directive ask:** "confirm asset/network vs layout
before big digs." **Answer: it is TEXT/RENDERING, not asset/network.** No
external images exist — the "images" are emoji glyphs; the cards are CSS
gradients. Status: **21.44 @ t15** (threshold 10, ceiling 22.4), unmoved this
session — the tractable half is fixed for flow contexts (#50) but the case is
`display:grid`, so it stays parked pending a grid change.

---

## What diverges (Chrome baseline vs RustKit capture, 1280×800)

The 4-column grid geometry, gaps, and card gradients all match well. Two
rendering gaps drive the 21.44:

### 1. Captions missing entirely — `POSITIONED SUBTREE` (root-caused; grid path parked)

Each card: `.gallery-item { position:relative; overflow:hidden }` containing
`.image-overlay { position:absolute; bottom:0 }` with an `<h3>` + `<p>`.

Root cause: layout places children at the flow origin, then
`apply_position_offsets` moves the positioned box **but not its subtree**
(content coords are absolute). The abs overlay's text laid out below the card
and `overflow:hidden` clipped it → captions vanish. Same disease hit
`position:relative` text.

- **Fixed for flow/block/flex contexts** in PR #50 (`apply_position_offsets`
  now translates the subtree by the origin delta; 245 unit tests + campaign
  24/26 + holdout 6/6 all green; falsification fixture
  `parity-tests/repro/positioned-subtree-translate.html`).
- **STILL BROKEN for grid** — `.gallery` is `display:grid`, and grid item
  child re-layout (`grid.rs` "Phase 9", ~L1942) **`continue`s past abs/fixed
  grandchildren** — they never get positioned against the grid item. A first
  attempt (re-`layout()` the abs grandchild against the grid item box when the
  item is positioned) compiled and regressed nothing but did **not** render the
  caption — so there is a THIRD layer (overlay height resolving to ~0, or the
  gallery-item not taking the block re-layout branch). Needs a dedicated grid
  dig with a layout-json probe of `.image-overlay`'s computed rect. Reverted to
  keep #50 proven-only.

### 2. Emoji render as gradient placeholder squares — `EMOJI FONT`

`🏔️ 🌅 🏖️ …` (font-size 3em, centered in `.image-placeholder`) paint as small
gradient squares, not glyphs. Color-emoji font path gap. Likely the larger
score contributor (one wrong glyph per card, card-center). Separate from #1.

---

## Recommended next order

1. **Grid abs-child positioning** (`grid.rs` Phase 9) — probe `.image-overlay`
   rect first; likely overlay height=0 or wrong re-layout branch. Pairs with #50.
2. **Emoji rendering** — bigger score lever here but font-path work; measure the
   emoji-only delta (mask captions) to size it before committing.
3. Re-measure; threshold is a strict 10, so both are likely needed to clear.

Diagnosis deliverable (asset/network vs layout) is **done: layout/rendering**.

— Atlas
