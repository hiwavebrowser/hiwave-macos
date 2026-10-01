# RustKit refactor plan

**Status:** approved by Pete 2026-09-23 · **Author seat:** a dedicated Grok agent (not Prometheus) · **Reviewers:** Prometheus (R1) + Cursor R2 gate bot · **Merger:** Prometheus

## Goal

Make the engine cheap and safe for agents to work in. The target is **smaller
files with one job each**. Line count is not the target: an agent chasing it
will golf code into something denser and worse. Every PR in this plan must
leave behaviour **exactly** unchanged.

## Measured starting point (develop `ec02a7f`, 2026-09-23)

| File | Lines | Inline tests start at | Test share |
|---|---:|---:|---:|
| `crates/rustkit-engine/src/lib.rs` | 14,697 | 10,391 | 29% |
| `crates/rustkit-layout/src/lib.rs` | 12,903 | 7,604 | 41% |
| `crates/rustkit-renderer/src/lib.rs` | 7,315 | 6,134 | 16% |
| `crates/rustkit-layout/src/grid.rs` | 7,222 | 3,444 | 52% |
| `crates/rustkit-layout/src/flex.rs` | 5,032 | 2,332 | 54% |
| `crates/hiwave-app/src/main.rs` | 4,482 | — | 0% |

About 17,000 lines of these files are inline `#[cfg(test)]` modules. The
compiler reports only 5 dead-code warnings across engine/layout/css. Public
items nobody calls are invisible to that check (see Phase 3).

Inside the two `lib.rs` files, the bulk is a single `impl` block:
`impl Engine` (lines 583–8552, about 8,000 lines) and `impl LayoutBox`
(1372–5171, about 3,800 lines), plus `impl DisplayList` (5928–7604).

## The behaviour-preservation rule (every PR, no exceptions)

A refactor PR may merge only if **all** of these hold at its head SHA,
measured against the develop it is based on:

1. `cargo test --workspace` passes, with **the same number of tests** (a moved
   test still runs, and none are dropped).
2. `python3 scripts/parity_test.py`: **26/26 byte-identical** per-case
   `diffPercent` to the base. Not "close". Identical.
3. `python3 scripts/wpt_tier1.py`: identical pass/fail set.
4. `cargo clippy` introduces no new warnings.
5. The PR does one kind of change (move / split / delete / simplify) and says
   which in its title: `refactor(move): …`, `refactor(split): …`,
   `refactor(dead): …`, `refactor(simplify): …`.

If one pixel moves, it is not a refactor. Back it out, or re-file it as a
`fix:` with its own review.

The R2 gate enforces 1–3 from the numbers in the PR body. Prometheus enforces 5.

## Windows: never refactor alongside feature work

Moves of thousands of lines conflict with every open PR. Refactoring runs only
in a **window**:

- Opens when the open-PR queue on the target crate is **empty or ≤ 2**, and
  Pete or Atlas declares it on the exchange.
- The real-site trench (`com.alephnull.trench-realsite`) is **paused** for the
  window: `launchctl bootout gui/$(id -u)/com.alephnull.trench-realsite`,
  re-bootstrap after.
- One crate per window. Land fast; aim for under 24h per window.
- Closes with a note on the exchange naming the new module layout, so every
  other seat rebases once.

First window: right after the current stack (#213, #214, #217, #215, #212,
#210) lands.

## Phases (in order; each PR ≤ ~2,000 lines moved)

### Phase 1 — move inline tests out (zero risk, about 17k lines)

For each file above, move each `#[cfg(test)] mod <name> { … }` into
`src/tests/<name>.rs` (or `src/<module>/tests.rs`), leaving
`#[cfg(test)] mod <name>;`. Pure cut-and-paste plus `use super::*` fixes.
One file per PR. Expected result: `rustkit-layout/src/lib.rs` drops to about
7,600 lines, `rustkit-engine/src/lib.rs` to about 10,400, and
`grid.rs`/`flex.rs` roughly halve.

### Phase 2 — split the giant impls into modules (pure moves)

Rust allows several `impl` blocks for one type across a crate, and child
modules can see the parent's private fields. So these are moves, not
redesigns:

- **rustkit-engine:** `impl Engine` split by responsibility into
  `engine/style.rs` (cascade: `compute_style_for_element`, declaration
  arms, `ch`/font resolution), `engine/selectors.rs` (`SimpleSelector`,
  matching), `engine/fonts.rs` (`FontShorthand`, `FontSource`, web fonts),
  `engine/load.rs` (`load_url`, `load_html`, `load_subresources`),
  `engine/events.rs` (input, forms, link clicks, scroll), and
  `engine/layout_build.rs` (DOM to LayoutBox). Keep `lib.rs` as types,
  `Engine` struct, builder, and `mod` declarations.
- **rustkit-layout:** `impl LayoutBox` into `block.rs`, `inline.rs`,
  `position.rs` (abspos, floats, stacking), `baseline.rs` (the seating
  helpers: `blink_baseline_offset`, `half_leading`, `seat_metrics`);
  `DisplayList` and paint into `paint/mod.rs` (+ `paint/text.rs`,
  `paint/borders.rs`, `paint/backgrounds.rs`).
- **rustkit-renderer**, **grid.rs**, **flex.rs**: after Phase 1, re-measure
  and split only what is still > 2,500 lines, along the spec's own sections
  (e.g. grid: track sizing / placement / alignment).

Rule of thumb: target ≤ 1,500 lines per file, and no file over 2,500.

### Phase 3 — delete dead code

- Compiler: fix every `dead_code` / `unused` warning (5 today).
- Unused public items: for every `pub` fn/struct in the rustkit crates, find
  callers across the workspace (Aleph `aleph_callers`, confirmed by
  `cargo check` after deletion). Delete items with zero non-test callers.
  **Keep** anything the engine exposes to `hiwave-app`, `parity-capture`, or
  `hiwave-mcp`.
- Dead feature paths: code behind a flag nothing enables (check `Cargo.toml`
  features and `cfg` attributes).
- One crate per PR. List every deleted item in the body.

### Phase 4 — simplify (last, smallest, most scrutiny)

Only where the result is clearly easier to read: duplicated helpers merged
(for example, the six sites that once computed half-leading independently),
long `match` arms extracted, `clone()`s removed. Every simplify PR still
meets the byte-identical rule. Never simplify and fix in the same PR.

## What we track (the digest line, per window)

- The largest file's line count, and the count of files over 2,500 lines.
- Dead-code warnings, and `pub` items with zero callers.
- Test count (must never drop).

These are the progress signal. The merge gate is only the behaviour-preservation rule.

## Out of scope

Renaming public API used across the HiWave repos, changing algorithms,
performance work, formatting-only churn (`cargo fmt` on develop currently
reformats 11 files; if wanted, that is one standalone `chore(fmt)` PR in a
window, never mixed into a refactor).
