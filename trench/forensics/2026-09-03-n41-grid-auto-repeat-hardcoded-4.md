# n41 — `repeat(auto-fit, …)` was hardcoded to 4 tracks; the auto-repeat layout code was dead

Night 41 (2026-09-03), macOS seat. Lane per the n40 digest's decision-2
condition: #174/#173 had not landed, so option (b) — about re-table + the
features-grid column placement (`div.features` cells in cards 4/5 at dx ≈ +486).

## The symptom

About page, cards 4 and 5, `.features`:

```css
.features { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 12px; }
```

Container content width 622px (both engines agree). Chrome 148 (pinned
baseline `layout-rects.json`): three columns of 199.3px at x = 89 / 300.3 /
511.7. RustKit: four columns of 150px at x = 89 / 251 / 413 / 575, the 1fr
max never applied, and every fourth cell landing where Chrome puts the first
cell of the next row (that is the +486).

## Two candidate bugs, one probe

Reading `rustkit-layout/src/grid.rs` gave two plausible mechanisms:

1. `calculate_auto_repeat_tracks` fits `floor((W + gap) / (track + gap))`
   repetitions — with W = 622, gap = 12, track = 150 that is **3**, not 4.
   The code is right.
2. `size_grid_tracks` Step 3 sized every fr track from one hypothetical unit
   and floored it at base: with four tracks, (622 − 36) / 4 = 146.5 < 150, so
   each stayed at 150. Wrong per css-grid-1 §12.7.1, but only visible once
   there are four tracks.

Neither explains **four**. `RUST_LOG=rustkit_layout=trace` on the repro
settled it in one run: the `auto-repeat: N repetitions fit` trace line never
fired. The layout crate's auto-repeat path was not running at all.

## Root cause

`rustkit-engine/src/lib.rs::parse_grid_template`:

```rust
let count: Option<u32> = if count_str == "auto-fill" || count_str == "auto-fit" {
    // For now, default to a reasonable number
    Some(4)
} else { count_str.parse().ok() };
```

The parser expanded every `auto-fill`/`auto-fit` to four copies of the track
and returned `repeats: Vec::new()`. `GridLayout::new` → `expand_tracks` →
`extract_auto_repeat` → `expand_auto_repeats` — all written, unit-tested, and
unreachable from a stylesheet. Every real page using the responsive-grid
idiom got exactly four columns at every viewport.

Aleph's index matched disk for all of this; the trap was that the layout
code *read* correct, so the temptation was to re-derive its math. A trace
probe on the running binary is cheaper than a second reading.

## Fixes (one PR, rustkit-engine + rustkit-layout)

1. **Parser** — `parse_grid_template` tokenizes the track list at top level
   (`split_track_list`: function arguments and `[line names]` stay whole),
   expands fixed-count `repeat()` inline, emits `auto-fill`/`auto-fit` as a
   `TrackRepeat` at its insert position, and attaches `[names]` to the
   following track (trailing group → `final_line_names`). Old parser: any
   `[name]` token silently vanished, and a track list containing a repeat
   dropped every track outside the repeat.
2. **fr sizing** (§12.7.1 "find the size of an fr") — a flexible track whose
   share falls below its base size is treated as inflexible at that base and
   the fr is re-found over the rest. `minmax(150px, 1fr) 1fr` in 200px: 150 +
   50, not 150 + 100 (overflow).
3. **Content-box grid items** (found by the repro, second commit) — Phase 8
   treated the grid area as the item's *content* size when `box-sizing:
   content-box`, then added padding on top, so a padded item overflowed its
   track by its padding in both axes (repro: 223.3 wide in a 199.3 track).
   The alignment helpers return the specified size — explicit `width` verbatim,
   or the area for `auto` — so the content box is now: explicit + content-box
   → as given; otherwise area − padding − border (css-grid-1 §6.6 stretch fills
   the area with the margin box regardless of box-sizing). Board cases all use
   a border-box reset, which is why this never showed on the meter.

## Receipts

- `parity-tests/repro/grid-auto-fit-minmax.html` (five grids, 622px, with
  gap / with column-gap+row-gap / auto-fill / no gap / two items):
  after fix 1+2, three 199.33px columns at x = 20 / 231.3 / 442.7 in every
  gapped grid; no-gap grid four at 155.5 (unchanged, was already right).
- About `.features` cells vs pinned Chrome (`scratch_n41/features_table.py`):
  every cell x / w / h matches (89 / 300.3 / 511.7, w 199.3, h 77 in card 4).
  Card 5's cells are h 47 vs Chrome 54: the emoji line (n40, PR #179, not on
  this branch).
- Tests: rustkit-layout 311/311 (3 new: auto-fit-with-gap end to end, fr
  re-find, content-box item); rustkit-engine parser test (auto-fit is a
  repeat, counts expand inline, insert position, line names).
- Campaign board (basis: n39 clean develop `5b89ed8`, unchanged since):
  fix 1+2 — 26/26, avg 4.0404 → 4.0336, new_tab 2.2600 → 2.0819 (−0.18pp),
  25 of 26 byte-flat. About is byte-flat: both `.features` grids sit at
  y ≈ 800–1200 in an 800×600 capture, below the fold — the n40 digest said
  so and the lane was taken with that known. WPT Tier-1 24/26 flat.
  Fix 3 board: identical — 26/26 avg 4.0336, same single mover (new_tab),
  25 of 26 byte-flat; WPT 24/26 flat. Repro cell after fix 3: 199.3 × 40.5,
  the border box equals the track in both axes (was 223.3 × 64.5).

## Ledger (not chased tonight)

- `apply_justify_self` / `apply_align_self` position End/Center items by the
  specified width, so a content-box item with an explicit width and padding
  is offset by its padding+border when right/center aligned.
- An auto-width item under a non-stretch `justify-self` still takes the full
  area width instead of fit-content.
- `repeat()` with a fixed count inside a longer track list and `[names]` on
  the repeat boundary are parsed now, but `repeat()` nested inside
  `minmax()` is not (not valid CSS anyway).
