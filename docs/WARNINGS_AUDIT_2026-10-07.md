# Compile-warnings audit — 2026-10-07 (Plan Z, slice W5-E)

**Question:** of all compiler warnings in this workspace, which point at real issues and which can simply be removed?

**Method.** On a Linux container (rustc/clippy 1.97.0, `x86_64-unknown-linux-gnu`), from `origin/develop`:

```bash
cargo check  --workspace --exclude rustkit-bench --exclude hiwave-app --exclude hiwave-smoke --all-targets --keep-going --message-format=json
cargo check  -p rustkit-bench --lib --bins --tests --examples --message-format=json   # its bench target is broken, see A0
cargo clippy (same two invocations)
```

Warnings were de-duplicated by (lint, file:line, message), so a lib warning that also fires in the
`--tests` build counts once. "clippy-only" means the warnings clippy adds on top of rustc's.
Platform-specific code was not compiled on Linux, and where a warning depended on that I used a
second pass: `cargo check --target aarch64-apple-darwin --all-targets` on the 25 crates whose C
dependencies allow it without a macOS SDK. That pass is rustc-only (no link and no clippy), and it
cannot reach `rustkit-engine`, which is blocked behind `ring`/`aws-lc-sys`/`libsqlite3-sys`.
I used it only to decide whether an item is dead on macOS as well, never to invent warnings.

Line numbers are at this PR's head, except in the "Fixed in this PR" table, which uses develop's.

## Headline

| | before | after this PR |
|---|---:|---:|
| rustc warnings (Linux, crates that build) | **90** | **82** |
| clippy-only warnings | **222** | **128** |
| total | **312** | **210** |
| rustc warnings in the macOS cross-check (25 crates, see above) | 17 | 10 |

Classification of the 312: **(A) real issue: 8 warnings** (6 findings; none fixed here) ·
**(B) noise: 214** (102 fixed here, 112 left, each with a reason) · **(C) intentional / platform-gated: 90**.
The PR also records one **build error** that is not a warning (A0).

There are no unused `Result`/`#[must_use]` values, no unreachable code, no unused `unsafe`, no
deprecated-API warnings and no clippy `correctness` hits (that group is deny-by-default and nothing
errored). Most of the rustc count (65 of the 82 left) is in `rustkit-engine` and is an artefact of
Linux: helpers and pages of test modules whose `#[test]`s are gated `cfg(target_os = "macos")`.

## Crate build table

| Crate | Status on Linux | Notes |
|---|---|---|
| hiwave-app | **FAILED-ON-LINUX** | `wry`/`tao` pull GTK/WebKitGTK system libraries (`atk-sys`, `gdk-sys`, `gdk-pixbuf-sys`, `pango-sys`, `soup3-sys`, `javascriptcore-rs-sys`, `wayland-sys` build scripts fail). macOS-only app; its warnings were not collected. |
| hiwave-smoke | **FAILED-ON-LINUX** | Same `wry`/`tao` dependency chain. Not collected. |
| rustkit-bench | BUILT (partial) | `--all-targets` fails: `[[bench]] rustkit` points at `../../benches/rustkit.rs`, which does not exist (A0). lib/bins/tests checked; 3/3 tests pass. |
| rustkit-media | BUILT | Needed `libasound2-dev` (ALSA, through `rodio`) installed in the container; otherwise the `alsa-sys` build script fails. |
| rustkit-viewhost | BUILT (Linux paths only) | The `macos.rs` / Win32 paths are cfg'd out on Linux. The macOS cross-check reached it (rustc-only). |
| rustkit-renderer | BUILT (Linux paths only) | The Metal-specific paths are cfg'd out. The macOS cross-check reached it. |
| rustkit-compositor | BUILT (Linux paths only) | The macOS surface creation is cfg'd out. The macOS cross-check reached it. |
| rustkit-engine | BUILT (Linux paths only) | Most test modules are `cfg(all(target_os = "macos", feature = "headless"))`. The macOS cross-check is blocked by C dependencies. |
| hiwave-core, hiwave-shell, hiwave-shield, hiwave-vault, hiwave-analytics, hiwave-mcp, parity-capture, rustkit-common, rustkit-core, rustkit-dom, rustkit-css, rustkit-cssparser, rustkit-layout, rustkit-js, rustkit-bindings, rustkit-net, rustkit-image, rustkit-test, rustkit-html, rustkit-http, rustkit-codecs, rustkit-animation, rustkit-svg, rustkit-canvas, rustkit-webgl, rustkit-idb, rustkit-sw, rustkit-worker, rustkit-text, rustkit-a11y | BUILT | check + clippy + test, all targets. |

None was SKIPPED. Not checked anywhere in this audit: `hiwave-app` and `hiwave-smoke` on any target,
and the Windows-only (`cfg(windows)`) code in every crate.

## Counts per crate and per lint

| Crate | rustc before | rustc after | clippy-only before | clippy-only after |
|---|---:|---:|---:|---:|
| rustkit-engine | 66 | 65 | 56 | 26 |
| rustkit-layout | 9 | 6 | 49 | 34 |
| rustkit-renderer | 5 | 4 | 40 | 35 |
| rustkit-html | 0 | 0 | 10 | 4 |
| rustkit-svg | 0 | 0 | 10 | 3 |
| rustkit-css | 2 | 0 | 7 | 1 |
| rustkit-animation | 0 | 0 | 7 | 6 |
| rustkit-net | 0 | 0 | 6 | 4 |
| rustkit-bindings | 2 | 2 | 3 | 0 |
| rustkit-canvas | 0 | 0 | 5 | 3 |
| rustkit-image | 0 | 0 | 5 | 2 |
| rustkit-a11y | 0 | 0 | 4 | 3 |
| rustkit-viewhost | 2 | 2 | 2 | 0 |
| rustkit-js | 1 | 0 | 2 | 1 |
| rustkit-text | 1 | 1 | 2 | 1 |
| rustkit-idb | 1 | 1 | 1 | 0 |
| rustkit-sw | 0 | 0 | 2 | 0 |
| rustkit-webgl | 0 | 0 | 2 | 2 |
| rustkit-worker | 0 | 0 | 2 | 0 |
| hiwave-analytics | 0 | 0 | 1 | 0 |
| hiwave-mcp | 0 | 0 | 1 | 1 |
| hiwave-shell | 0 | 0 | 1 | 1 |
| hiwave-shield | 0 | 0 | 1 | 0 |
| rustkit-compositor | 1 | 1 | 0 | 0 |
| rustkit-core | 0 | 0 | 1 | 0 |
| rustkit-cssparser | 0 | 0 | 1 | 1 |
| rustkit-http | 0 | 0 | 1 | 0 |
| **total** | **90** | **82** | **222** | **128** |

| Lint | before | after |
|---|---:|---:|
| `dead_code` | 70 | 66 |
| `clippy::field_reassign_with_default` | 34 | 34 |
| `clippy::too_many_arguments` | 18 | 18 |
| `clippy::manual_strip` | 16 | 10 |
| `clippy::derivable_impls` | 15 | 0 |
| `clippy::clone_on_copy` | 13 | 0 |
| `clippy::manual_clamp` | 13 | 13 |
| `unused_imports` | 13 | 13 |
| `clippy::excessive_precision` | 12 | 12 |
| `clippy::unnecessary_map_or` | 12 | 0 |
| `clippy::manual_pattern_char_comparison` | 9 | 0 |
| `clippy::type_complexity` | 8 | 8 |
| `clippy::manual_is_multiple_of` | 6 | 6 |
| `clippy::unnecessary_sort_by` | 5 | 4 |
| `clippy::useless_vec` | 5 | 0 |
| `clippy::bool_assert_comparison` | 4 | 0 |
| `clippy::collapsible_if` | 4 | 0 |
| `clippy::manual_div_ceil` | 4 | 4 |
| `clippy::only_used_in_recursion` | 4 | 4 |
| `clippy::double_ended_iterator_last` | 3 | 0 |
| `clippy::get_first` | 3 | 0 |
| `clippy::redundant_closure` | 3 | 0 |
| `clippy::should_implement_trait` | 3 | 3 |
| `unused_variables` | 3 | 1 |
| `clippy::collapsible_match` | 2 | 2 |
| `clippy::doc_lazy_continuation` | 2 | 0 |
| `clippy::int_plus_one` | 2 | 0 |
| `clippy::manual_range_contains` | 2 | 1 |
| `clippy::manual_split_once` | 2 | 0 |
| `non_snake_case` | 2 | 2 |
| `unused_mut` | 2 | 0 |
| `clippy::assertions_on_constants` | 1 | 1 |
| `clippy::empty_line_after_doc_comments` | 1 | 0 |
| `clippy::io_other_error` | 1 | 0 |
| `clippy::items_after_test_module` | 1 | 1 |
| `clippy::let_and_return` | 1 | 0 |
| `clippy::manual_find` | 1 | 0 |
| `clippy::manual_ignore_case_cmp` | 1 | 0 |
| `clippy::match_like_matches_macro` | 1 | 1 |
| `clippy::needless_option_as_deref` | 1 | 0 |
| `clippy::needless_range_loop` | 1 | 1 |
| `clippy::neg_cmp_op_on_partial_ord` | 1 | 1 |
| `clippy::redundant_guards` | 1 | 1 |
| `clippy::replace_box` | 1 | 1 |
| `clippy::result_unit_err` | 1 | 1 |
| `clippy::single_match` | 1 | 0 |
| `clippy::unnecessary_cast` | 1 | 0 |
| `clippy::unnecessary_to_owned` | 1 | 0 |
| `clippy::wildcard_in_or_patterns` | 1 | 1 |

## (A) Real issues — not fixed in this PR

| # | Where | Warning | Why it matters | Suggested fix |
|---|---|---|---|---|
| A0 | `crates/rustkit-bench/Cargo.toml` (`[[bench]] rustkit`) | *build error, not a warning:* `can't find bench rustkit at path …/benches/rustkit.rs` | `cargo check/clippy/test --workspace --all-targets` and `cargo bench` fail outright for the whole workspace. The benchmark crate has no benchmark, so nothing measures performance. | Restore `benches/rustkit.rs` (or move it into `crates/rustkit-bench/benches/`), or delete the `[[bench]]` table. Then `--all-targets` works as a workspace gate again. |
| A1 | `crates/rustkit-renderer/src/lib.rs:445`, `:455`, `:470` (`QueuedLinearGradient` / `QueuedRadialGradient` / `QueuedConicGradient`: every field never read) and `:3372` (`render_linear_gradient_gpu` never used) | `dead_code` ×4 (the method shares the `:1287` warning with A2) | **A GPU gradient path that was never wired.** `draw_*_gradient` push into `gradient_queue` / `radial_gradient_queue` / `conic_gradient_queue` (`:4001`, `:4276`, `:4407`), but nothing drains them. The comment at `:627-646` already forces `gpu_gradients_enabled = false`, because honouring `RUSTKIT_GPU_GRADIENTS` deleted every gradient. Today it is dead, inert weight; the danger is someone flipping the flag back on. | Either wire it (the caller that should exist is `execute_with_gpu_gradients` / the flush that `render_linear_gradient_gpu_with_clear` serves: drain the three queues there, and add radial and conic GPU draws), or delete the queues, the structs and `render_linear_gradient_gpu`, together with the env flag. Decide alongside the GPU-gradients disarm record (PR133). |
| A2 | `crates/rustkit-renderer/src/lib.rs:1287` (`run_color_filter_compute` never used) | `dead_code` (same warning as A1's method) | **A compute path that is built but never called.** `backdrop_filter_pipelines.color_filter_pipeline` is compiled at renderer start-up, but `apply_backdrop_filter` (`:3263`) paints `grayscale()` / `sepia()` / `brightness()` as flat colour overlays ("isn't accurate but provides visual feedback", `:3308`). Every `backdrop-filter` other than blur is therefore approximate, and the shader is wasted start-up work. | The caller that should exist is `apply_backdrop_filter`'s Grayscale / Sepia / Brightness arms (Blur already uses `run_blur_compute`). It needs the render-to-texture split described at `:3236-3262`. Until then, drop the pipeline or keep it behind a targeted `#[allow(dead_code)]` that points at that plan. |
| A3 | `crates/rustkit-layout/src/lib.rs:1826` `update_sticky_positions(&mut self, scroll_x, …)` | `clippy::only_used_in_recursion` (`scroll_x`) | **`position: sticky` is vertical-only.** `scroll_x` is threaded through the whole tree and never read. `StickyState::update` (`crates/rustkit-layout/src/scroll.rs:638`) takes only `scroll_y` ("Simplified sticky: only handle top sticky"). `left`/`right` sticky (horizontal scrollers, sticky first columns) and `bottom` sticky do nothing. | Pass `scroll_x` into `StickyState::update` and handle the `left`/`right` (and `bottom`) offsets. Add a parity case with a horizontally scrolling sticky column. |
| A4 | `crates/rustkit-idb/src/lib.rs:77` `RequestId::new` | `dead_code` | **An IndexedDB request model with no requests.** `RequestId` is public, but nothing mints one, `IDBEvent` carries only `db_name`, and no crate depends on `rustkit-idb`, so `window.indexedDB` is not exposed. Any two concurrent `open()`s on one database cannot be told apart. | When IDB is wired to bindings, give each `IDBFactory::open` / `deleteDatabase` / store operation a `RequestId::new()`, carry it in `IDBEvent`, and route events by it. Until then the crate is a stub; say so in its docs. |
| A5 | `crates/rustkit-html/src/tree_builder.rs:225` (`new_fragment`) | `clippy::wildcard_in_or_patterns` (`"body" \| "div" \| "span" \| "p" \| _`) | **A dead pattern that hides missing spec arms.** The named strings are documentation only. The fragment-parsing algorithm (HTML §13.4, *reset the insertion mode*) also maps `caption` → in caption, `colgroup` → in column group, `tbody`/`thead`/`tfoot`/`tr`/`td`/`th` (present) and `head`/`html` (present). `caption` and `colgroup` contexts fall through to InBody, so `innerHTML` on a `<caption>` or `<colgroup>` parses with the wrong rules. | Add the `caption` → `InsertionMode::InCaption` and `colgroup` → `InsertionMode::InColumnGroup` arms (both modes already exist, `tree_builder.rs:60-61`), and make the fallback a bare `_`. |
| A6 | `crates/rustkit-layout/src/grid.rs:7334` (`test_grid_items_filter_display_none`) | `clippy::assertions_on_constants` (`assert!(true)`) | **A test that tests nothing.** It always passes. The comment says the behaviour is "tested by verifying the filter exists", which no code does, so a regression that placed `display: none` items into grid tracks would go unnoticed. | Build a grid with a `display: none` child and assert that it occupies no track and that the auto-placement cursor skips it. |

Lower-priority observations (no warning, or not counted above):

- `crates/rustkit-engine/src/lib.rs:305-306`: a stray `/// View state.` doc line and `#[allow(dead_code)]` sit on `SubresourceReferrer` (which is used); they look left over from a moved `ViewState`. Harmless, but the `allow` hides future dead fields there.
- `crates/rustkit-layout/src/text.rs:1356` (`TextLayout::is_default_ignorable`) and `:2738` (free `is_default_ignorable`) are two copies of the same code-point table. They will drift.
- `crates/rustkit-test/src/lib.rs:22` has a crate-level `#![allow(dead_code, unused_imports)]`, the kind of blanket allow this audit's rules forbid. It hides an unknown number of warnings. 74 targeted `#[allow(dead_code)]` across the workspace also sit outside this count.

## (B) Noise

### Fixed in this PR (102 warnings, no behaviour change)

rustc (8):

| Location (develop) | Lint | Change |
|---|---|---|
| `crates/rustkit-css/src/lib.rs:4049`, `:4072` | `unused_variables` | `auto_repeat` → `_auto_repeat` in two tests. |
| `crates/rustkit-js/src/import_map.rs:121` | `dead_code` | Removed `ImportMap::is_empty` (private module; no caller on any target). |
| `crates/rustkit-layout/src/lib.rs:2533` | `dead_code` | `layout_image` is only called from tests → `#[cfg(test)]`. |
| `crates/rustkit-layout/src/lib.rs:3021` | `dead_code` | Removed `text_baseline_extents` (superseded by `text_line_box_extents`; dead on macOS too). |
| `crates/rustkit-layout/src/lib.rs:3655` | `dead_code` | Removed `definite_content_height` (superseded by `definite_content_height_for_children` / `inset_definite_content_height`; dead on macOS too). One doc comment that named it now says "on `LayoutBox`". |
| `crates/rustkit-layout/src/lib.rs:13696`, `crates/rustkit-renderer/src/lib.rs:6662` | `unused_mut` | Dropped `mut`. |
| `crates/rustkit-engine/src/lib.rs:317` | `dead_code` | Removed `SubresourceReferrer::get` (all callers use `get_for`; grep finds no use on any cfg). Its doc moved onto `get_for`. |

clippy (94), applied with `cargo clippy --fix` restricted to a whitelist of machine-applicable lints
whose rewrite is semantically identical, then reviewed hunk by hunk and hand-formatted. The repo is
not rustfmt-clean, so whole-file `cargo fmt` would have added noise; only `hiwave-shield/src/filter_lists.rs`
was already rustfmt-clean, and it was formatted. The 94 by lint: `derivable_impls` 15 (manual `impl Default`
→ `#[derive(Default)]` + `#[default]` variant), `clone_on_copy` 13, `unnecessary_map_or` 12
(`map_or(true, …)` → `is_none_or`, `map_or(false, …)` → `is_some_and`; `is_none_or` is already used on
develop), `manual_pattern_char_comparison` 9, `manual_strip` 6, `useless_vec` 5 (tests), `collapsible_if` 4,
`bool_assert_comparison` 4, `get_first` 3, `redundant_closure` 3, `double_ended_iterator_last` 3
(`.last()` → `.next_back()` / `.rfind()`; every closure in those chains is pure), `int_plus_one` 2 (test),
`manual_split_once` 2, `doc_lazy_continuation` 2 + `empty_line_after_doc_comments` 1 (doc text only; the
latter was an orphaned `/// Build a layout tree from a DOM document.` glued onto
`transfer_positioning`), `unnecessary_sort_by` 1 (stable sort either way), and one each of
`io_other_error`, `let_and_return`, `manual_find`, `manual_ignore_case_cmp`, `needless_option_as_deref`,
`single_match`, `unnecessary_cast`, `unnecessary_to_owned`, and `manual_range_contains` (the test in
`rustkit-renderer/src/dither.rs`, where `a <= d && d <= b` and `(a..=b).contains(&d)` agree even for NaN).

Auto-fixes I **reverted** after review:
- `collapsible_match` at `rustkit-engine/src/lib.rs:11221` and `rustkit-html/src/tree_builder.rs:1975`. Both are equivalent today only because the `_` arm they fall into is empty. Moving the condition into a guard makes the next edit to that `_` arm a silent behaviour change, and the code reads worse.
- `manual_range_contains` at `rustkit-renderer/src/lib.rs:4050`. `angle < 90 || angle > 270` is false for NaN, but `!(90..=270).contains(&angle)` is true, so the rewrite is not behaviour-free.
- `excessive_precision` ×12 (moved to C).

### Left as is (112 warnings): style-only, but the fix is not mechanical or not provably behaviour-free

| Lint | n | Why it was not fixed here |
|---|---:|---|
| `clippy::field_reassign_with_default` | 34 | No machine-applicable fix; needs a hand rewrite to struct-literal + `..Default::default()` (23 in `rustkit-layout/src/lib.rs` tests). Safe follow-up. |
| `clippy::too_many_arguments` | 18 | Needs parameter structs (API shape). The repo convention elsewhere is a targeted `#[allow]` (19 exist). |
| `clippy::manual_clamp` | 13 | **Not behaviour-free:** `f32::clamp` panics when `min > max` or a bound is NaN, and `max().min()` chains do not. 8 are in `rustkit-renderer/src/glyph.rs`. Check the bounds before converting. |
| `clippy::manual_strip` | 10 | Clippy marks these "maybe incorrect" (index arithmetic differs from the prefix length). Convert by hand. |
| `clippy::type_complexity` | 8 | Needs type aliases. Cosmetic. |
| `clippy::manual_is_multiple_of` | 6 | **Not behaviour-free:** `x % 0` panics, while `x.is_multiple_of(0)` returns `x == 0`. |
| `clippy::manual_div_ceil` | 4 | **Not behaviour-free on overflow:** `(a + b - 1) / b` overflows where `div_ceil` does not. |
| `clippy::unnecessary_sort_by` | 4 | Not machine-applicable (key borrows). Trivial by hand. |
| `clippy::only_used_in_recursion` | 3 | `rustkit-engine/src/lib.rs:10851` (`keys_for_compound`), `:27551` (`SubjectCompound::parse`), `:27621` (`matches`): an unused `engine`/`SelectorMatcher` parameter. Dropping it changes private signatures and call sites; a trivial follow-up. (The fourth hit is A3.) |
| `clippy::should_implement_trait` | 3 | `rustkit-a11y` `from_str` methods; implementing `FromStr` changes the public API. |
| `clippy::collapsible_match` | 2 | Reverted, see above. |
| `clippy::manual_range_contains` | 1 | Reverted (NaN), see above. |
| `clippy::redundant_guards` | 1 | `rustkit-engine/src/lib.rs:15566`: `Px(v) if v == 0.0` → a float literal pattern. Equivalent, but float patterns are a readability trap; leave it. |
| `clippy::items_after_test_module`, `needless_range_loop`, `replace_box`, `result_unit_err`, `match_like_matches_macro` | 5 | One each; cosmetic, no machine-applicable fix. |

## (C) Intentional / platform-gated: keep, ideally with a targeted `#[allow]` or a matching `cfg`

This PR does **not** add these `#[allow]`s (it is B-only). The recommended treatment is below.

| Where | Warning(s) | Why it is intentional | Recommended treatment |
|---|---|---|---|
| `rustkit-engine`: `src/grid_block_axis_align_tests.rs`, `grid_flexible_row_tests.rs`, `grid_item_abspos_tests.rs`, `grid_item_lone_text_tests.rs`, `grid_item_min_max_tests.rs`, `place_shorthand_tests.rs`; `src/lib.rs` test modules `scroll_wiring_tests`, `button_children_tests`, `svg_image_tests`, `web_font_tests`, `node_identity_tests` (`:22350-24300`), `form_typing_tests`, `form_submit_tests`, `scroll_extent_tests` and others | 50 `dead_code` (pages, `BOXES`/`GAPS` consts, helpers) + 11 `unused_imports` (`use super::*;`) | Every `#[test]` that uses them is `#[cfg(all(target_os = "macos", feature = "headless"))]` (or `target_os = "macos"`). The helpers are ungated, so on Linux they are dead; on macOS they are used. Removing them would break the macOS build. | Put the same `cfg` on the `mod` declaration (all tests in each module share it) or on each helper. That is better than an `#[allow]`, and it needs a macOS compile to verify, so it is left for a macOS slice. |
| `rustkit-engine/src/lib.rs:6687` `create_pseudo_element`, `:28163` `RuleIndexScope::install` | `dead_code` | `#[cfg(test)]` helpers used only by macOS-gated tests (`:25750+`, `:25715+`). | Same as above: `cfg(all(test, target_os = "macos", feature = "headless"))`. |
| `rustkit-engine/src/lib.rs:469` `ViewState::{id, view_focused}` | `dead_code` | `id` is read in the macOS `create_view`; `view_focused` is read only by the Windows key path (`handle_key_event`, `cfg(windows)`). | `#[cfg_attr(not(windows), allow(dead_code))] // read only by the Win32 key path` on `view_focused`; nothing for `id` (Linux-only noise). |
| `rustkit-engine/src/lib.rs:194` `WindowHandle` | `unused_imports` | Used only by the `cfg(target_os = "macos")` / `cfg(windows)` `create_view`. | Gate the import: `#[cfg(any(target_os = "macos", windows))]`. |
| `rustkit-layout/src/shaped_run_tests.rs:10-65` (5 helpers), `rustkit-layout/src/text.rs:2738` `is_default_ignorable` | `dead_code` ×6 | Used only from macOS-gated code (confirmed by the macOS cross-check, where they do not warn). | Matching `cfg`. |
| `rustkit-text/src/webfonts.rs:246` `bump_generation` | `dead_code` | Called from `macos.rs:786` and the platform install path; dead only on Linux (no warning on macOS). | `#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]`. |
| `rustkit-compositor/src/lib.rs:114` `Compositor::instance` | `dead_code` | Read only by the macOS/Windows surface-creation paths (`:232`, `:302`); no warning on macOS. | Same `cfg_attr` pattern. |
| `rustkit-viewhost/src/lib.rs:41` `warn`, `:955` `hwnd_raw` | `unused_imports`, `unused_variables` | Used only in `cfg(windows)` / macOS code; neither warns on macOS. Linux is not a viewhost target. | `cfg`-gate the import and the binding. |
| `rustkit-bindings/src/web_components_tests.rs:52`, `:99` | `non_snake_case` | The test names quote the DOM API they pin (`whenDefined`, `createElement`). | `#[allow(non_snake_case)] // named after the DOM method under test` on those two fns. |
| `rustkit-renderer/src/lib.rs:4519-4552` | `clippy::excessive_precision` ×12 | The OKLab matrices are Björn Ottosson's published coefficients. Truncating them to the f32-representable digits changes no value but loses the link to the reference. | `#[allow(clippy::excessive_precision)] // published OKLab coefficients, kept verbatim` on the function. |
| `rustkit-layout/src/lib.rs:189` | `clippy::neg_cmp_op_on_partial_ord` | `!(ratio > 0.0)` deliberately rejects NaN as well as non-positive values; `ratio <= 0.0` would let NaN through. | `#[allow(clippy::neg_cmp_op_on_partial_ord)] // NaN must fail this test`. |

## Test evidence (Linux, before and after this PR)

`cargo test --workspace --exclude hiwave-app --exclude hiwave-smoke --exclude rustkit-bench --no-fail-fast`
plus `cargo test -p rustkit-bench --lib --bins --tests`:

| | suites | tests | passed | failed | ignored |
|---|---:|---:|---:|---:|---:|
| before (develop) | 76 + 1 | 2216 + 3 | 1970 + 3 | 242 | 4 |
| after (this PR) | 76 + 1 | 2216 + 3 | 1970 + 3 | 242 | 4 |

The per-test outcome lists are **identical**. The 242 failures predate this PR and come from the container:
239 in `rustkit-engine` lib tests panic with `No suitable GPU adapter found` (no GPU in the
container), and 3 in `rustkit-layout` (`bare_control_widths_match_chrome`,
`justified_wrapped_lines_fill_the_container_except_the_last`,
`a_long_first_run_keeps_its_last_line_open_for_the_next_sibling`) are font-metric pins that need macOS fonts.
The macOS cross-check (`--target aarch64-apple-darwin`, 25 crates, rustc only) went from 17 to 10 warnings,
with no new warning and no error.

## Appendix — every remaining warning (210), with its bucket

| Bucket | Crate | Lint | Location | Message |
|---|---|---|---|---|
| A | rustkit-html | `clippy::wildcard_in_or_patterns` | `crates/rustkit-html/src/tree_builder.rs:225` | wildcard pattern covers any other pattern as it will match anyway |
| A | rustkit-idb | `dead_code` | `crates/rustkit-idb/src/lib.rs:77` | associated function `new` is never used |
| A | rustkit-layout | `clippy::assertions_on_constants` | `crates/rustkit-layout/src/grid.rs:7334` | this assertion is always `true` |
| A | rustkit-layout | `clippy::only_used_in_recursion` | `crates/rustkit-layout/src/lib.rs:1826` | parameter is only used in recursion |
| A | rustkit-renderer | `dead_code` | `crates/rustkit-renderer/src/lib.rs:1287` | methods `run_color_filter_compute` and `render_linear_gradient_gpu` are never used |
| A | rustkit-renderer | `dead_code` | `crates/rustkit-renderer/src/lib.rs:446` | fields `rect`, `angle_rad`, `stops`, `repeating`, and `border_radius` are never read |
| A | rustkit-renderer | `dead_code` | `crates/rustkit-renderer/src/lib.rs:456` | multiple fields are never read |
| A | rustkit-renderer | `dead_code` | `crates/rustkit-renderer/src/lib.rs:471` | fields `rect`, `from_angle_rad`, `center`, `stops`, `repeating`, and `border_radius` are never read |
| B | hiwave-mcp | `clippy::items_after_test_module` | `crates/hiwave-mcp/src/main.rs:526` | items after a test module |
| B | hiwave-shell | `clippy::unnecessary_sort_by` | `crates/hiwave-shell/src/lib.rs:618` | consider using `sort_by_key` |
| B | rustkit-a11y | `clippy::should_implement_trait` | `crates/rustkit-a11y/src/lib.rs:156` | method `from_str` can be confused for the standard trait method `std::str::FromStr::from_str` |
| B | rustkit-a11y | `clippy::should_implement_trait` | `crates/rustkit-a11y/src/lib.rs:287` | method `from_str` can be confused for the standard trait method `std::str::FromStr::from_str` |
| B | rustkit-a11y | `clippy::should_implement_trait` | `crates/rustkit-a11y/src/lib.rs:319` | method `from_str` can be confused for the standard trait method `std::str::FromStr::from_str` |
| B | rustkit-animation | `clippy::manual_clamp` | `crates/rustkit-animation/src/lib.rs:257` | clamp-like pattern without using clamp function |
| B | rustkit-animation | `clippy::manual_is_multiple_of` | `crates/rustkit-animation/src/lib.rs:782` | manual implementation of `.is_multiple_of()` |
| B | rustkit-animation | `clippy::manual_is_multiple_of` | `crates/rustkit-animation/src/lib.rs:789` | manual implementation of `.is_multiple_of()` |
| B | rustkit-animation | `clippy::manual_is_multiple_of` | `crates/rustkit-animation/src/lib.rs:809` | manual implementation of `.is_multiple_of()` |
| B | rustkit-animation | `clippy::manual_is_multiple_of` | `crates/rustkit-animation/src/lib.rs:816` | manual implementation of `.is_multiple_of()` |
| B | rustkit-animation | `clippy::too_many_arguments` | `crates/rustkit-animation/src/lib.rs:1063` | this function has too many arguments (8/7) |
| B | rustkit-canvas | `clippy::too_many_arguments` | `crates/rustkit-canvas/src/lib.rs:1090` | this function has too many arguments (10/7) |
| B | rustkit-canvas | `clippy::too_many_arguments` | `crates/rustkit-canvas/src/lib.rs:362` | this function has too many arguments (9/7) |
| B | rustkit-canvas | `clippy::too_many_arguments` | `crates/rustkit-canvas/src/lib.rs:923` | this function has too many arguments (9/7) |
| B | rustkit-css | `clippy::needless_range_loop` | `crates/rustkit-css/src/lib.rs:1562` | the loop variable `row` is used to index `rows` |
| B | rustkit-cssparser | `clippy::type_complexity` | `crates/rustkit-cssparser/src/lib.rs:951` | very complex type used. Consider factoring parts into `type` definitions |
| B | rustkit-engine | `clippy::collapsible_match` | `crates/rustkit-engine/src/lib.rs:11221` | this `if` can be collapsed into the outer `match` |
| B | rustkit-engine | `clippy::field_reassign_with_default` | `crates/rustkit-engine/src/lib.rs:29483` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-engine | `clippy::field_reassign_with_default` | `crates/rustkit-engine/src/lib.rs:29893` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-engine | `clippy::field_reassign_with_default` | `crates/rustkit-engine/src/lib.rs:30366` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-engine | `clippy::field_reassign_with_default` | `crates/rustkit-engine/src/lib.rs:34471` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-engine | `clippy::manual_is_multiple_of` | `crates/rustkit-engine/src/lib.rs:12054` | manual implementation of `.is_multiple_of()` |
| B | rustkit-engine | `clippy::manual_strip` | `crates/rustkit-engine/src/lib.rs:15398` | stripping a suffix manually |
| B | rustkit-engine | `clippy::manual_strip` | `crates/rustkit-engine/src/lib.rs:15403` | stripping a suffix manually |
| B | rustkit-engine | `clippy::manual_strip` | `crates/rustkit-engine/src/lib.rs:15704` | stripping a suffix manually |
| B | rustkit-engine | `clippy::manual_strip` | `crates/rustkit-engine/src/lib.rs:15709` | stripping a suffix manually |
| B | rustkit-engine | `clippy::manual_strip` | `crates/rustkit-engine/src/lib.rs:15714` | stripping a suffix manually |
| B | rustkit-engine | `clippy::manual_strip` | `crates/rustkit-engine/src/lib.rs:15716` | stripping a suffix manually |
| B | rustkit-engine | `clippy::manual_strip` | `crates/rustkit-engine/src/lib.rs:15965` | stripping a prefix manually |
| B | rustkit-engine | `clippy::manual_strip` | `crates/rustkit-engine/src/lib.rs:15981` | stripping a prefix manually |
| B | rustkit-engine | `clippy::only_used_in_recursion` | `crates/rustkit-engine/src/lib.rs:10851` | parameter is only used in recursion |
| B | rustkit-engine | `clippy::only_used_in_recursion` | `crates/rustkit-engine/src/lib.rs:27551` | parameter is only used in recursion |
| B | rustkit-engine | `clippy::only_used_in_recursion` | `crates/rustkit-engine/src/lib.rs:27621` | parameter is only used in recursion |
| B | rustkit-engine | `clippy::redundant_guards` | `crates/rustkit-engine/src/lib.rs:15566` | redundant guard |
| B | rustkit-engine | `clippy::too_many_arguments` | `crates/rustkit-engine/src/lib.rs:6960` | this function has too many arguments (9/7) |
| B | rustkit-engine | `clippy::too_many_arguments` | `crates/rustkit-engine/src/script_net.rs:347` | this function has too many arguments (8/7) |
| B | rustkit-engine | `clippy::type_complexity` | `crates/rustkit-engine/src/lib.rs:27948` | very complex type used. Consider factoring parts into `type` definitions |
| B | rustkit-engine | `clippy::type_complexity` | `crates/rustkit-engine/src/lib.rs:28054` | very complex type used. Consider factoring parts into `type` definitions |
| B | rustkit-engine | `clippy::type_complexity` | `crates/rustkit-engine/src/lib.rs:28153` | very complex type used. Consider factoring parts into `type` definitions |
| B | rustkit-engine | `clippy::type_complexity` | `crates/rustkit-engine/src/lib.rs:3404` | very complex type used. Consider factoring parts into `type` definitions |
| B | rustkit-engine | `clippy::type_complexity` | `crates/rustkit-engine/src/lib.rs:3833` | very complex type used. Consider factoring parts into `type` definitions |
| B | rustkit-engine | `clippy::unnecessary_sort_by` | `crates/rustkit-engine/src/lib.rs:12119` | consider using `sort_by_key` |
| B | rustkit-html | `clippy::collapsible_match` | `crates/rustkit-html/src/tree_builder.rs:1975` | this `if` can be collapsed into the outer `match` |
| B | rustkit-html | `clippy::manual_strip` | `crates/rustkit-html/src/entities.rs:350` | stripping a prefix manually |
| B | rustkit-html | `clippy::match_like_matches_macro` | `crates/rustkit-html/src/tokenizer.rs:1138` | match expression looks like `matches!` macro |
| B | rustkit-image | `clippy::type_complexity` | `crates/rustkit-image/src/lib.rs:881` | very complex type used. Consider factoring parts into `type` definitions |
| B | rustkit-image | `clippy::type_complexity` | `crates/rustkit-image/src/lib.rs:922` | very complex type used. Consider factoring parts into `type` definitions |
| B | rustkit-js | `clippy::unnecessary_sort_by` | `crates/rustkit-js/src/import_map.rs:134` | consider using `sort_by_key` |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/flex.rs:3066` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:10729` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:10749` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:11411` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:11675` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:11743` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:11817` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:11975` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:12037` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:12074` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:12138` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:12162` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:12286` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:12348` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:12420` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:12472` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:12501` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:12564` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:14103` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:15887` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:15967` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:16026` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:16062` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/lib.rs:16085` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::field_reassign_with_default` | `crates/rustkit-layout/src/multicol.rs:184` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-layout | `clippy::replace_box` | `crates/rustkit-layout/src/flex.rs:3624` | creating a new box |
| B | rustkit-layout | `clippy::too_many_arguments` | `crates/rustkit-layout/src/images.rs:29` | this function has too many arguments (8/7) |
| B | rustkit-layout | `clippy::too_many_arguments` | `crates/rustkit-layout/src/margin_collapse.rs:208` | this function has too many arguments (8/7) |
| B | rustkit-layout | `clippy::too_many_arguments` | `crates/rustkit-layout/src/text.rs:2070` | this function has too many arguments (8/7) |
| B | rustkit-layout | `clippy::too_many_arguments` | `crates/rustkit-layout/src/text.rs:2126` | this function has too many arguments (8/7) |
| B | rustkit-layout | `clippy::too_many_arguments` | `crates/rustkit-layout/src/text.rs:2647` | this function has too many arguments (10/7) |
| B | rustkit-net | `clippy::field_reassign_with_default` | `crates/rustkit-net/src/lib.rs:1097` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-net | `clippy::manual_strip` | `crates/rustkit-net/src/cache.rs:456` | stripping a prefix manually |
| B | rustkit-net | `clippy::result_unit_err` | `crates/rustkit-net/src/security.rs:351` | this returns a `Result<_, ()>` |
| B | rustkit-net | `clippy::unnecessary_sort_by` | `crates/rustkit-net/src/intercept.rs:147` | consider using `sort_by_key` |
| B | rustkit-renderer | `clippy::manual_clamp` | `crates/rustkit-renderer/src/glyph.rs:373` | clamp-like pattern without using clamp function |
| B | rustkit-renderer | `clippy::manual_clamp` | `crates/rustkit-renderer/src/glyph.rs:374` | clamp-like pattern without using clamp function |
| B | rustkit-renderer | `clippy::manual_clamp` | `crates/rustkit-renderer/src/glyph.rs:523` | clamp-like pattern without using clamp function |
| B | rustkit-renderer | `clippy::manual_clamp` | `crates/rustkit-renderer/src/glyph.rs:524` | clamp-like pattern without using clamp function |
| B | rustkit-renderer | `clippy::manual_clamp` | `crates/rustkit-renderer/src/glyph.rs:549` | clamp-like pattern without using clamp function |
| B | rustkit-renderer | `clippy::manual_clamp` | `crates/rustkit-renderer/src/glyph.rs:550` | clamp-like pattern without using clamp function |
| B | rustkit-renderer | `clippy::manual_clamp` | `crates/rustkit-renderer/src/glyph.rs:638` | clamp-like pattern without using clamp function |
| B | rustkit-renderer | `clippy::manual_clamp` | `crates/rustkit-renderer/src/glyph.rs:639` | clamp-like pattern without using clamp function |
| B | rustkit-renderer | `clippy::manual_clamp` | `crates/rustkit-renderer/src/lib.rs:2777` | clamp-like pattern without using clamp function |
| B | rustkit-renderer | `clippy::manual_clamp` | `crates/rustkit-renderer/src/lib.rs:2829` | clamp-like pattern without using clamp function |
| B | rustkit-renderer | `clippy::manual_clamp` | `crates/rustkit-renderer/src/lib.rs:2872` | clamp-like pattern without using clamp function |
| B | rustkit-renderer | `clippy::manual_clamp` | `crates/rustkit-renderer/src/lib.rs:8277` | clamp-like pattern without using clamp function |
| B | rustkit-renderer | `clippy::manual_div_ceil` | `crates/rustkit-renderer/src/lib.rs:1252` | manually reimplementing `div_ceil` |
| B | rustkit-renderer | `clippy::manual_div_ceil` | `crates/rustkit-renderer/src/lib.rs:1253` | manually reimplementing `div_ceil` |
| B | rustkit-renderer | `clippy::manual_div_ceil` | `crates/rustkit-renderer/src/lib.rs:1314` | manually reimplementing `div_ceil` |
| B | rustkit-renderer | `clippy::manual_div_ceil` | `crates/rustkit-renderer/src/lib.rs:1315` | manually reimplementing `div_ceil` |
| B | rustkit-renderer | `clippy::manual_range_contains` | `crates/rustkit-renderer/src/lib.rs:4050` | manual `!RangeInclusive::contains` implementation |
| B | rustkit-renderer | `clippy::too_many_arguments` | `crates/rustkit-renderer/src/lib.rs:1635` | this function has too many arguments (9/7) |
| B | rustkit-renderer | `clippy::too_many_arguments` | `crates/rustkit-renderer/src/lib.rs:1710` | this function has too many arguments (8/7) |
| B | rustkit-renderer | `clippy::too_many_arguments` | `crates/rustkit-renderer/src/lib.rs:3504` | this function has too many arguments (8/7) |
| B | rustkit-renderer | `clippy::too_many_arguments` | `crates/rustkit-renderer/src/lib.rs:3654` | this function has too many arguments (9/7) |
| B | rustkit-renderer | `clippy::too_many_arguments` | `crates/rustkit-renderer/src/lib.rs:3785` | this function has too many arguments (8/7) |
| B | rustkit-renderer | `clippy::too_many_arguments` | `crates/rustkit-renderer/src/lib.rs:4152` | this function has too many arguments (8/7) |
| B | rustkit-svg | `clippy::field_reassign_with_default` | `crates/rustkit-svg/src/lib.rs:2624` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-svg | `clippy::field_reassign_with_default` | `crates/rustkit-svg/src/lib.rs:2636` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-svg | `clippy::field_reassign_with_default` | `crates/rustkit-svg/src/lib.rs:2649` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-text | `clippy::manual_is_multiple_of` | `crates/rustkit-text/src/bidi.rs:69` | manual implementation of `.is_multiple_of()` |
| B | rustkit-webgl | `clippy::field_reassign_with_default` | `crates/rustkit-webgl/src/lib.rs:590` | field assignment outside of initializer for an instance created with Default::default() |
| B | rustkit-webgl | `clippy::too_many_arguments` | `crates/rustkit-webgl/src/lib.rs:1066` | this function has too many arguments (10/7) |
| C | rustkit-bindings | `non_snake_case` | `crates/rustkit-bindings/src/web_components_tests.rs:52` | function `whenDefined_resolves_when_the_name_is_defined` should have a snake case name |
| C | rustkit-bindings | `non_snake_case` | `crates/rustkit-bindings/src/web_components_tests.rs:99` | function `new_and_createElement_make_a_defined_element_that_connects_on_insertion` should have a snake case name |
| C | rustkit-compositor | `dead_code` | `crates/rustkit-compositor/src/lib.rs:114` | field `instance` is never read |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/grid_block_axis_align_tests.rs:21` | constant `BOXES` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/grid_block_axis_align_tests.rs:40` | constant `GAPS` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/grid_block_axis_align_tests.rs:47` | function `boxes` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/grid_flexible_row_tests.rs:23` | constant `BOXES` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/grid_flexible_row_tests.rs:32` | constant `CONTROL_LINE_GAPS` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/grid_flexible_row_tests.rs:41` | constant `LONE_IMAGE_GAPS` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/grid_item_abspos_tests.rs:19` | constant `BOXES` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/grid_item_abspos_tests.rs:38` | constant `GAPS` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/grid_item_lone_text_tests.rs:16` | constant `BOXES` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/grid_item_lone_text_tests.rs:28` | constant `GAPS` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/grid_item_lone_text_tests.rs:30` | function `boxes` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/grid_item_min_max_tests.rs:18` | constant `BOXES` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/grid_item_min_max_tests.rs:31` | constant `GAPS` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:21622` | function `widest_inline_block` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:21631` | function `probe_width` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:22358` | function `js` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:22368` | constant `DRAG_CLICK_PAGE` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:22425` | constant `FOCUS_PRESS_PAGE` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:22592` | constant `HOVER_PAGE` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23035` | constant `POINTER_PAGE` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23295` | function `painted_checks` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23308` | constant `ACTIVATION_PAGE` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23322` | function `activation_page` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23344` | function `js_string` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23348` | function `painted` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23478` | constant `FRAGMENT_PAGE` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23492` | function `fragment_page` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23611` | constant `SUBMIT_PAGE` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23624` | function `submit_page` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23643` | function `click_row` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23715` | constant `FOCUS_PAGE` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23725` | function `focus_page` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23742` | function `focused_id` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23829` | constant `DISABLED_PAGE` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23940` | constant `LABEL_PAGE` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:23999` | constant `KEYS_PAGE` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:24006` | function `keys_page` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:24090` | constant `SHORTCUT_PAGE` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:24096` | function `shortcut_page` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:24184` | constant `DETAILS_PAGE` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:24194` | function `details_page` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:24212` | function `link_rows` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:24222` | function `rows` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:24281` | function `pump` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:24290` | constant `ADD_LINK` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:28163` | associated function `install` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:34786` | constant `SCRIPT_GAPS` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:34791` | constant `WHEEL_GAPS` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:469` | fields `id` and `view_focused` are never read |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/lib.rs:6687` | method `create_pseudo_element` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/place_shorthand_tests.rs:19` | constant `BOXES` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/place_shorthand_tests.rs:31` | constant `LONGHAND_GAPS` is never used |
| C | rustkit-engine | `dead_code` | `crates/rustkit-engine/src/place_shorthand_tests.rs:33` | function `boxes` is never used |
| C | rustkit-engine | `unused_imports` | `crates/rustkit-engine/src/grid_flexible_row_tests.rs:20` | unused import: `super::*` |
| C | rustkit-engine | `unused_imports` | `crates/rustkit-engine/src/grid_item_abspos_tests.rs:15` | unused import: `super::*` |
| C | rustkit-engine | `unused_imports` | `crates/rustkit-engine/src/grid_item_min_max_tests.rs:14` | unused import: `super::*` |
| C | rustkit-engine | `unused_imports` | `crates/rustkit-engine/src/lib.rs:194` | unused import: `WindowHandle` |
| C | rustkit-engine | `unused_imports` | `crates/rustkit-engine/src/lib.rs:20761` | unused import: `super::*` |
| C | rustkit-engine | `unused_imports` | `crates/rustkit-engine/src/lib.rs:20826` | unused import: `super::*` |
| C | rustkit-engine | `unused_imports` | `crates/rustkit-engine/src/lib.rs:21386` | unused import: `super::*` |
| C | rustkit-engine | `unused_imports` | `crates/rustkit-engine/src/lib.rs:24385` | unused import: `super::*` |
| C | rustkit-engine | `unused_imports` | `crates/rustkit-engine/src/lib.rs:24561` | unused import: `super::*` |
| C | rustkit-engine | `unused_imports` | `crates/rustkit-engine/src/lib.rs:24677` | unused import: `super::*` |
| C | rustkit-engine | `unused_imports` | `crates/rustkit-engine/src/lib.rs:24754` | unused import: `super::*` |
| C | rustkit-engine | `unused_imports` | `crates/rustkit-engine/src/lib.rs:34773` | unused import: `super::*` |
| C | rustkit-layout | `clippy::neg_cmp_op_on_partial_ord` | `crates/rustkit-layout/src/lib.rs:189` | the use of negated comparison operators on partially ordered types produces code that is hard to read and refactor, please consider using the `partial_cmp` method instead, to make it clear that the two values could be incomparable |
| C | rustkit-layout | `dead_code` | `crates/rustkit-layout/src/shaped_run_tests.rs:10` | function `style_in` is never used |
| C | rustkit-layout | `dead_code` | `crates/rustkit-layout/src/shaped_run_tests.rs:20` | function `kerning_corpus` is never used |
| C | rustkit-layout | `dead_code` | `crates/rustkit-layout/src/shaped_run_tests.rs:46` | function `one_line_list` is never used |
| C | rustkit-layout | `dead_code` | `crates/rustkit-layout/src/shaped_run_tests.rs:58` | type alias `TextParts` is never used |
| C | rustkit-layout | `dead_code` | `crates/rustkit-layout/src/shaped_run_tests.rs:65` | function `text_parts` is never used |
| C | rustkit-layout | `dead_code` | `crates/rustkit-layout/src/text.rs:2738` | function `is_default_ignorable` is never used |
| C | rustkit-renderer | `clippy::excessive_precision` | `crates/rustkit-renderer/src/lib.rs:4519` | float has excessive precision |
| C | rustkit-renderer | `clippy::excessive_precision` | `crates/rustkit-renderer/src/lib.rs:4520` | float has excessive precision |
| C | rustkit-renderer | `clippy::excessive_precision` | `crates/rustkit-renderer/src/lib.rs:4521` | float has excessive precision |
| C | rustkit-renderer | `clippy::excessive_precision` | `crates/rustkit-renderer/src/lib.rs:4529` | float has excessive precision |
| C | rustkit-renderer | `clippy::excessive_precision` | `crates/rustkit-renderer/src/lib.rs:4530` | float has excessive precision |
| C | rustkit-renderer | `clippy::excessive_precision` | `crates/rustkit-renderer/src/lib.rs:4531` | float has excessive precision |
| C | rustkit-renderer | `clippy::excessive_precision` | `crates/rustkit-renderer/src/lib.rs:4540` | float has excessive precision |
| C | rustkit-renderer | `clippy::excessive_precision` | `crates/rustkit-renderer/src/lib.rs:4541` | float has excessive precision |
| C | rustkit-renderer | `clippy::excessive_precision` | `crates/rustkit-renderer/src/lib.rs:4542` | float has excessive precision |
| C | rustkit-renderer | `clippy::excessive_precision` | `crates/rustkit-renderer/src/lib.rs:4550` | float has excessive precision |
| C | rustkit-renderer | `clippy::excessive_precision` | `crates/rustkit-renderer/src/lib.rs:4551` | float has excessive precision |
| C | rustkit-renderer | `clippy::excessive_precision` | `crates/rustkit-renderer/src/lib.rs:4552` | float has excessive precision |
| C | rustkit-text | `dead_code` | `crates/rustkit-text/src/webfonts.rs:246` | function `bump_generation` is never used |
| C | rustkit-viewhost | `unused_imports` | `crates/rustkit-viewhost/src/lib.rs:41` | unused import: `warn` |
| C | rustkit-viewhost | `unused_variables` | `crates/rustkit-viewhost/src/lib.rs:955` | unused variable: `hwnd_raw` |
