Night block 42 (macOS trench seat), from develop `5b89ed8`. Shared crate: rustkit-layout only (`flex.rs`, one visibility change in `grid.rs`). Forensics: `trench/forensics/2026-09-04-n42-flex-item-padding-twice-and-column-guess.md`.

**Two bugs, one lane (flex-positioning 10.11 → 1.75, the board's biggest unclaimed case):**

1. **An auto-width padded flex item was max-content + its padding twice.** `create_flex_item` adds `main_pb` to `get_intrinsic_main_size()`, whose Block arm returned `estimate_max_content_width` — a border-box figure that already folds `horizontal_padding_border` in. `.flex-item { padding: 10px 20px }` measured 123.2 vs Chrome 83.1 (+40), `.justify-item` +30, `.nested-item` +24.7. The arm now takes the estimator's own padding term back out (`horizontal_padding_border` made `pub(crate)`).
2. **A column container placed its rows on a one-line guess and never revisited it.** The vertical intrinsic main size is a line height; 11b/11c corrected the cross axis after child layout, nothing corrected the main axis, so rows overlapped whenever one was taller than a text line (row 2 at y=534 with row 1 ending at 537; Chrome 548). New step 11d re-derives content-sized items' main sizes from the laid-out height, re-runs grow/shrink + justification against the container's own definite height or `max(content, min-height)` (min-height resolved from style incl. `vh` — the engine passes a column container its stale pre-pass stacked height as the containing block, which is what develop centred new_tab's body in), and translates subtrees. Same pass fixes 11b writing a sum of child *heights* into a column item's *width* (new_tab `.container` was 745 wide, max-width 600).

**Receipts**
- T-RED: both new tests fail on the unfixed code with the board's numbers (content width 83.2 vs 43.2; row 2 on the guess).
- rustkit-layout 308 → 312 tests (4 new). `cargo test -p rustkit-layout` green.
- Repro `parity-tests/repro/flex-item-padding-column-basis.html` vs pinned Chrome 148: rows (border-box and content-box), column of rows, column of wrapped paragraphs, definite-height column with flex-grow — containers and items within 1px (ledger: a nested row grown by 11d does not re-stretch its own items).
- Campaign board (n39 clean develop basis, unchanged): 26/26, **avg 4.0404 → 3.6097**; flex-positioning −8.37pp, shelf −0.58, gradient-backgrounds −0.84, gradient-no-radius −0.96, gradient-radius-only −0.60, chrome_rustkit −0.09; **new_tab +0.28** — its `.container` goes from (y +282, w +145 vs Chrome) to (y −6, w exact): the shortcuts grid, still 4 hardcoded columns until #182 lands, used to sit half below the fold and now sits where Chrome's 2-column grid is. 17/26 byte-flat. WPT Tier-1 24/26 flat.

**Real-page reach:** every padded nav link / pill / tab / button in a flex row was too wide by its padding and pushed its siblings right; every column layout with rows taller than a line overlapped them; `min-height: 100vh` centring columns centred in a stale number.

Independent of #174/#173/#176/#179/#182 (different functions; #176 is `lib.rs` shrink-to-fit, #182 is `grid.rs` Step 3 / Phase 8 and the engine parser). Honest last-run WPT pin on the exact tip.
