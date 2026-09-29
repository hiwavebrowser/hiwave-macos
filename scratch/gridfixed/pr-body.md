Real-site trench: grid items no longer grow fixed tracks. They overflow them, as css-grid-1 §12.5 says and Chrome does.

Branched from develop 8920e24. Two commits: the fix (7fe10c2) and a pin-strength follow-up (f277c67).

**Why.** `grid-template-columns: 10px 10px` holding the text "a" gave a first track of 11.12px in RustKit; Chrome keeps 10px and lets the text overflow. §12.5 sizes only intrinsic tracks (`auto`, `min-content`, `max-content`, `fit-content()`), so a fixed track's size never depends on its items. Two places broke that:
1. `distribute_span_contributions` had an "all tracks fixed: distribute equally anyway" branch, which grew fixed tracks whenever an item spanned only fixed tracks.
2. The post-layout row re-sizer in `layout_grid_container` grew any row whose items laid out taller than it, fixed or not.

**What.** A new `track_is_fixed(track)`: no intrinsic, flexible or percentage sizing function, and `growth_limit <= base_size`. (1) now skips such items (`continue`), and (2) leaves fixed rows alone.

**Tests** (rustkit-engine, both layout entry points: `layout()` and `layout_with_collapse`):
- a fixed column with overflowing text keeps its size: 36.45 → 10. **Fails without the fix.**
- a fixed row with a taller item keeps its size: 50 → 20. **Fails without the fix.**
- an item spanning only fixed columns doesn't grow them: 50 → 30. **Fails without the fix.**
- `auto` columns still grow to their content. A guard: passes before and after.

**Campaign receipt** (`scripts/parity_test.py`, default scope = the 21 micro cases + the 5 builtins: new_tab, about, settings, chrome_rustkit, shelf):
- This PR, run `2026-09-28T23:42:41` at head **f277c67**: **26/26 passed, avg diff_pct 1.1756%**.
- develop (binary built at 8567760), run `2026-09-28T23:39:38` back to back: 26/26, avg 1.1756%. **Every case is identical.**
- CI's gates run locally (hub `scratch/shelf302/ratchet_local.py`): Gate A + Gate B + ratchet, **"RATCHET holds: none worse than the committed floor"**. Same 23 absolutely-red cases as develop.

<details><summary>Per-case diff_pct (26 cases)</summary>

| case | develop | this PR |
|---|---|---|
| new_tab | 1.5685 | 1.5685 |
| about | 3.6742 | 3.6742 |
| settings | 2.0854 | 2.0854 |
| chrome_rustkit | 1.1234 | 1.1234 |
| shelf | 1.0781 | 1.0781 |
| article-typography | 4.7857 | 4.7857 |
| card-grid | 1.3042 | 1.3042 |
| css-selectors | 1.3819 | 1.3819 |
| flex-positioning | 0.6327 | 0.6327 |
| form-elements | 0.9875 | 0.9875 |
| gradient-backgrounds | 1.0133 | 1.0133 |
| image-gallery | 0.5217 | 0.5217 |
| sticky-scroll | 0.6575 | 0.6575 |
| backgrounds | 1.2163 | 1.2163 |
| bg-solid | 0.2106 | 0.2106 |
| bg-pure | 0.0000 | 0.0000 |
| combinators | 0.6547 | 0.6547 |
| form-controls | 3.2442 | 3.2442 |
| gradients | 0.1412 | 0.1412 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.6892 | 0.6892 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| images-intrinsic | 0.3267 | 0.3267 |
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 1.3221 | 1.3221 |
| specificity | 0.6015 | 0.6015 |

</details>

**Real-site A/B:** not run for this PR. The machine sat at load 15–36 for the whole window, and three lanes were building. Fixed grid tracks with overflowing items are rare on the board's first viewports (wikipedia's shell uses `minmax()`/`auto` tracks), so I expect ±0. That's an expectation, not a measurement, and the next session's full board will check it.

**Engine tests:** layout 554/554 at f277c67 (previous session). The engine suite is the same flake class as #345's parallel run, and the four new pins pass.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
