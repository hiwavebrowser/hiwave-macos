# CLAUDE.md

Guidance for every AI agent working in this repository: lead, implementer,
reviewer, batch agent or bot.

## Ownership

**This codebase is yours: Pete has given you the goals, you decide how to get
there in all aspects, and you'll keep building on it until it's done.**

What that means in practice:

- You do not wait to be told how. Pick the approach, measure it, land it.
- If you find a bug, a wrong test or a stale document while doing something
  else, fix it or file it as a row in the plan. Do not walk past it.
- Finish what you start. A branch nobody pushed and a PR nobody batched are
  lost work.
- Ownership is exercised through the method below (red test first, a pinned
  Chrome measurement, independent review, the batch gate). The gates are how
  we know a change is right; they are not permission slips from Pete.

Three things stay Pete's call. Ask him; do not decide them yourself:

1. **Goals and priorities.** The mission, what "done" means, and the current
   focus (which board or row comes first).
2. **Releases.** Promoting `develop` to `master`, and anything users download.
3. **Outward-facing acts.** Security advisories, public posts, licence
   changes, spending money, and loading live sites beyond the approved lists.

## Goals and "done"

The mission and its six pillars are in `MISSION.md`: a from-scratch browser in
Rust, as good as Chrome and better, private by default, outside Google's
ecosystem. Read it once.

"Done" is measured, not felt. The bars and where each is defined are in
`docs/GATES.md`:

| Bar | Done when | Tool |
|---|---|---|
| Chrome board (our own UI vs Chrome) | every row green | `scripts/chrome_board.py` |
| Real-site board (top sites vs Chrome) | every site loads, is readable and looks right, top 20 then top 100 | `scripts/realsite_board.py` (its baseline is on the `atlas/trench-realsite` branch) |
| Latency | the exit metric in `trench/latency/BASELINE.md` | `tools/latency/` |
| 100% RustKit | `wry`, `tao` and `muda` are deleted from the workspace | `Cargo.toml` |

The order between these bars is a priority, so it is Pete's. The current
order and the open rows live in the plan files: `trench/z/PLAN-z.md` and
`trench/devtools/PLAN-devtools.md` on the `atlas/z` branch of the
`hiwavebrowser/hiwave` hub repository. The plan file is
the project's memory: if it is not in the plan, it will be forgotten.

## Project overview

HiWave is a privacy-first browser for macOS built in Rust. Its engine,
**RustKit**, renders pages in pure Rust with GPU output through wgpu.
JavaScript is Boa (vendored and patched in `third_party/boa_engine`, see its
`PATCHES.md`). Ad and tracker blocking is Brave's adblock-rust.

On macOS, RustKit draws everything by default: page content and the browser's
own UI (tab bar, address bar, shelf, Inspector, settings). The system WebKit
view survives only as an escape hatch for the browser UI
(`HIWAVE_RUSTKIT_CHROME=0`). `tao` still provides the window and event loop
and `muda` the menu bar. The Windows and Linux ports live in their own
repositories (`hiwave-windows`, `hiwave-linux`) and sync from this one.

## Build and test

Rust toolchain is pinned in `rust-toolchain.toml`.

```bash
# Run the browser (the `rustkit` feature is on by default)
cargo run -p hiwave-app
cargo run -p hiwave-app --release

# Gate 0: must compile, with no new warnings, after every change
cargo check -p rustkit-engine --features headless
cargo check -p hiwave-app

# Engine tests (headless: offscreen GPU, no window)
cargo test -p rustkit-engine --lib --features headless
cargo test -p rustkit-engine --lib --features headless my_module_tests   # one module

# Other crates
cargo test -p rustkit-layout --lib        # likewise rustkit-css, rustkit-bindings, ...
cargo test -p hiwave-app --bin hiwave     # the app crate is bin-only: --bins, not --lib

# Script guards
for t in scripts/tests/test_*.py; do python3 "$t"; done

# Format
cargo fmt --all
```

Measurement builds use the `parity` profile (opt-level 3, no LTO):
`cargo build --profile parity -p parity-capture`.

### Rules for running tests

- **Every test run has a time limit.** macOS has no `timeout`; wrap the
  command in a wall-clock limiter. A hang is a finding: report it.
- **The Mac's CPU is the bottleneck.** While you implement, run gate 0 and
  the test filters for what you touched. The full suites run once on GitHub
  Actions and once in the batch. Do not run them a third time.
- **One heavy job at a time.** Serialise cargo across checkouts, and do not
  start a build or suite while the 1-minute load average is above 10.
- **Judge a run by its exit code**, not by the tail of its log.
- **Never rerun CI to fish for green**, and never push an empty commit to
  retrigger it. A red check is read, explained and fixed.

## Architecture

### Views

- **Content views**: RustKit. A web document gets no app bridge: no
  `window.ipc`, no host objects.
- **Chrome, shelf, Inspector, settings**: the browser's own pages in
  `crates/hiwave-app/src/ui/`, drawn by RustKit as app-owned views. They talk
  to Rust through `window.ipc.postMessage()`; Rust handles messages in
  `ipc/commands.rs` and answers with `evaluate_script()`.

App-owned pages are privileged. Anything a web page can influence (titles,
URLs, history, console lines) is untrusted when it reaches them: build DOM
with `textContent` and listeners, never markup or inline handlers from
strings. Logs carry shapes (command name, kind, size), never bodies.

### Workspace crates

**Application**
- `hiwave-app` - the app: event loop, IPC, chrome pages, RustKit integration
- `hiwave-core` - shared types (TabId, WorkspaceId, ...)
- `hiwave-shell` - tabs, workspaces, command palette
- `hiwave-shield` / `hiwave-shield-intercept` - ad and tracker blocking
- `hiwave-vault` - password manager
- `hiwave-analytics` - local-only usage analytics

**RustKit engine**
- `rustkit-engine` - orchestration, views, style cascade, page lifecycle
- `rustkit-core` / `rustkit-common` - runtime, shared utilities and errors
- `rustkit-dom`, `rustkit-html` - DOM tree, HTML parser
- `rustkit-css` / `rustkit-cssparser` - CSS values and parsing
- `rustkit-layout` - block, inline, flex, grid
- `rustkit-text` - shaping and glyph rasterisation
- `rustkit-renderer` / `rustkit-compositor` - GPU drawing, layers
- `rustkit-viewhost` - windows, surfaces, input
- `rustkit-js` / `rustkit-bindings` - Boa and the DOM bindings
- `rustkit-net` / `rustkit-http` - networking
- `rustkit-image` / `rustkit-codecs` / `rustkit-svg` - images, codecs, SVG
- `rustkit-animation`, `rustkit-canvas`, `rustkit-webgl`, `rustkit-media`
- `rustkit-worker`, `rustkit-sw`, `rustkit-idb`
- `rustkit-a11y`
- `rustkit-test` / `rustkit-bench`

**Tools**
- `parity-capture` - headless frame capture; every board and campaign uses it
- `hiwave-mcp` - MCP server exposing the engine to agents
- `tools/real_window/` - drives the built app in its real window
- `tools/parity_oracle/` - the Chrome side of every comparison
- `tools/latency/` - latency sessions and baselines

### Key files

| File | Purpose |
|---|---|
| `crates/hiwave-app/src/main.rs` | entry point, event loop, IPC routing |
| `crates/hiwave-app/src/webview_rustkit.rs` | RustKit content views, content thread |
| `crates/hiwave-app/src/webview_rustkit_chrome.rs` | RustKit chrome views |
| `crates/hiwave-app/src/ipc/` | IPC message types and handlers |
| `crates/hiwave-app/src/ui/` | the browser's own pages |
| `crates/hiwave-app/src/state.rs` | AppState, persistence |
| `crates/rustkit-engine/src/lib.rs` | engine orchestration, cascade, selector matching |
| `crates/rustkit-layout/src/lib.rs` | layout |
| `crates/rustkit-renderer/src/lib.rs` | drawing |
| `crates/rustkit-css/src/lib.rs` | CSS values |

### Feature flags (`hiwave-app`)

- `rustkit` (default) - RustKit views
- `test-hooks` - the real-window checks' panel probe in a release build.
  Never in a shipped build.
- `headless` (on `rustkit-engine`) - offscreen rendering for tests

## How a change gets to develop

1. **Take a row.** Work comes from the plan files. New findings become rows.
2. **Fresh checkout, own branch off `develop`.** One task per checkout;
   remove it after the push. Never work on `develop` or `master` directly.
3. **Measure Chrome first.** Expected values come from the pinned oracle,
   Chrome for Testing 148.0.7778.216, never from memory, the spec alone, or
   what RustKit does today. Say in the PR which Chrome produced each number.
4. **Red, then fix.** Commit the failing test, then the fix. A test that
   also passes on the old code proves nothing: show it red.
5. **Prove it where it runs.** A unit test is not proof for app behaviour;
   use the real-window driver for anything that goes through the event loop.
6. **Open a draft PR on `develop`.** State what was measured, what was not
   run, and what is left. Keep at most 16 PRs open across the fleet.
7. **Independent review (R1).** Someone who did not write it reviews it on
   GitHub: CLEAR, CLEAR WITH NITS, or HOLD. The review body on GitHub is the
   record. A new head needs a delta review. Take the nits or backlog them
   with a row; do not ignore them.
8. **The batch gate.** Review-clear PRs are merged together onto `develop`
   in a batch checkout and the whole tree is tested on the Mac (below).
   Implementers do not merge their own PRs.
9. **Landing.** Each PR is merged with `--match-head-commit`. The landed
   tree must equal the tested tree; verify the tree hash and say so.

### The batch gate

A batch merges the cleared PRs with `--no-ff`, compiling after each merge
(gate 0, no new warnings), then runs, in order:

1. the headless engine suite, then every crate's suite
2. the app crate three times, with no flake tolerance
3. script guards and tool tests
4. release, test-hooks and parity-capture builds
5. the parity campaign (`scripts/parity_test.py` cases vs Chrome)
6. the chrome board, both arms (develop and candidate)
7. the 19-site A/B (develop vs candidate; movers are rerun with arms swapped)
8. the WPT Tier-1 reftests (`scripts/wpt_tier1.py`)
9. the real-site board
10. two full real-window runs (`tools/real_window/driver.py`)

Rules of the gate:

- A red is read before it is rerun. Find the cause from the code and the
  log; rerun only to count a suspected flake (alone 20x, on develop 5x).
- A conflict between two PRs, or a test one PR invalidates in another, is
  resolved in the batch and the resolution is carried onto the PR branch at
  landing, so `develop` equals what was tested.
- A number that looks too good is checked before it is reported.
- The batch report says what was not verified.

### Real-window runs

The driver takes over the screen. Run it only while holding the real-window
lock and with the machine idle (no keyboard or mouse input for 10 minutes),
checked before every run. Any job that puts a headed browser in front takes
the same lock. Kill only processes your own run started.

## Hard rules

- **Git:** no force-push. Never `git add -A`; stage paths by name. Retarget
  stacked PRs before deleting their base branch. Do not delete a branch with
  unmerged work without the lead's go-ahead.
- **Security findings stay out of public text.** PR titles, bodies, review
  comments and commit messages describe the hardening, never the exploit.
  Details go to the lead privately.
- **Live sites:** no live-site loads except through the existing board and
  A/B tools and their site lists. Some sites are replay-only or excluded;
  the lists say which. No challenge solving, no faked APIs.
- **Privacy:** logs and reports carry shapes, not page content, passwords,
  usernames or URLs with secrets. No personal data in commits.
- **Honesty:** report what failed with its output, what was skipped, and
  what you did not check. Never claim a run you did not make.
- **No timeline promises.** Give order and dependencies, not dates.

## Pull requests labelled `no-check`

A pull request carrying the GitHub label `no-check` is a standing or parked PR
(for example a long-lived ledger). Do not review it, stamp it, run a receipt on
it, push to it, or merge it, and leave it out of review queues and status
lists. Check a PR's labels before starting work on it. Removing the label puts
it back in the queue. Only the project lead adds or removes `no-check`; a worker
does not park its own PR. A `no-check` PR's `r2-pass` label or earlier stamp is
not a pass for its current head.

## Commit messages

Conventional commits: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`,
`chore:`, `perf:`, `ci:`. Name the plan row when there is one
(`fix(layout): ... (CH2.B17b)`). A large diff carries a `large-diff:` line in
the PR body explaining why; write it yourself.

## Where things are

- `MISSION.md` - why this exists
- `docs/GATES.md` - every gate, its threshold, and the code that decides it
- `README.md` "Engine Status" - parity, WPT, known gaps
- `docs/` - current plans and design notes; `docs/archive/` is history only
- `docs/diagnostics/<date>/` - measurements and oracle outputs behind a fix
- `trench/` - baselines, digests and forensics for each long-running effort
- `fixtures/`, `websuite/` - test pages; `baselines/`, `goldens/` - reference frames
- `third_party/wpt/` - the WPT projection; not checked in, built by `scripts/wpt_sync.sh`
