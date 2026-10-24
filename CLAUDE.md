# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

HiWave is a privacy-first browser for macOS built in Rust. It features a custom browser engine called **RustKit** that provides pure-Rust rendering with GPU acceleration via wgpu, alongside native ad/tracker blocking via Brave's adblock-rust engine.

## Build Commands

```bash
# Build and run (the `rustkit` feature is on by default)
cargo run -p hiwave-app

# Release build
cargo run -p hiwave-app --release

# Quick syntax check (macOS; on Linux, wry needs the GTK/WebKitGTK dev packages)
cargo check -p hiwave-app

# What CI runs (macOS): compile every test target, then the per-crate suites
cargo test --workspace --no-run
cargo test -p rustkit-layout --lib        # likewise rustkit-engine, rustkit-js, ...
for t in scripts/tests/test_*.py; do python3 "$t"; done   # instrument guards

# Run tests
cargo test --workspace                    # All tests
cargo test -p hiwave-shield               # Specific crate
cargo test test_name -- --nocapture       # Single test with output

# Format and lint
cargo fmt --all
cargo clippy --workspace -- -D warnings
```

## Parity Testing (Visual Regression)

RustKit rendering is compared against Chrome baselines for pixel-level accuracy:

```bash
# Build parity capture tool
cargo build --release -p parity-capture

# Run parity test suite
python3 scripts/parity_test.py                     # All cases
python3 scripts/parity_test.py --scope builtins    # Built-in pages only
python3 scripts/parity_test.py --case new_tab      # Single case

# Run parity swarm (parallel testing)
python3 scripts/parity_swarm.py --jobs 4 --scope all

# Generate baselines from Chrome
python3 scripts/generate_baselines.py
```

## Architecture

### Three-WebView Model

The browser uses a multi-WebView architecture:
- **Chrome WebView** (WRY/WebKit): Browser UI, tabs, address bar, sidebar
- **Content WebView**: Web page rendering. RustKit on macOS; WRY on Windows/Linux
- **Shelf WebView** (WRY/WebKit): Command palette, collapsible panels

### Workspace Crates

**HiWave Application Crates:**
- `hiwave-app` - Main application, event loop, IPC handling
- `hiwave-core` - Shared types (TabId, WorkspaceId, etc.)
- `hiwave-shell` - Tab/workspace management, command palette
- `hiwave-shield` - Ad blocking engine (Brave's adblock-rust)
- `hiwave-vault` - Password manager (AES-256 encryption)
- `hiwave-analytics` - Local-only usage analytics

**RustKit Engine Crates:**
- `rustkit-engine` - Engine orchestration, multi-view management, style cascade
- `rustkit-core` / `rustkit-common` - Engine runtime, shared utilities and errors
- `rustkit-dom` - DOM tree representation
- `rustkit-html` - HTML parser
- `rustkit-css` / `rustkit-cssparser` - CSS parsing and styling
- `rustkit-layout` - Block/inline/flex/grid layout
- `rustkit-text` - Text shaping and rendering
- `rustkit-renderer` - GPU rendering via wgpu
- `rustkit-compositor` - Layer compositing
- `rustkit-viewhost` - Window/surface management
- `rustkit-js` / `rustkit-bindings` - JavaScript (Boa, vendored in `third_party/boa_engine`) and DOM bindings
- `rustkit-net` / `rustkit-http` - Network stack
- `rustkit-image` / `rustkit-codecs` / `rustkit-svg` - Images, codecs, SVG
- `rustkit-animation`, `rustkit-canvas`, `rustkit-webgl`, `rustkit-media` - CSS animations, Canvas 2D, WebGL, audio/video
- `rustkit-worker`, `rustkit-sw`, `rustkit-idb` - Web Workers, Service Workers, IndexedDB
- `rustkit-a11y` - Accessibility
- `rustkit-test` / `rustkit-bench` - WPT-style harness, benchmarks

**Tools:**
- `parity-capture` - Headless frame capture for parity testing
- `hiwave-mcp` - MCP server exposing the engine to agents (see `docs/HIWAVE_MCP_PLAN.md`)
- `hiwave-smoke` - Scripted smoke harness (macOS)

### Key Files

| File | Purpose |
|------|---------|
| `crates/hiwave-app/src/main.rs` | Entry point, event loop, WebView setup |
| `crates/hiwave-app/src/state.rs` | AppState, persistence, shelf logic |
| `crates/hiwave-app/src/ipc/` | IPC message types and command handlers |
| `crates/hiwave-app/src/ui/` | HTML/CSS/JS for browser UI |
| `crates/hiwave-app/src/webview_rustkit.rs` | RustKit content WebView integration |
| `crates/hiwave-app/src/shield_adapter.rs` | Ad blocking for RustKit requests |
| `crates/rustkit-engine/src/lib.rs` | Engine orchestration layer |

### Feature Flags

The `hiwave-app` crate uses feature flags:
- `rustkit` (default) - Use RustKit for content, WRY for chrome
- `headless` (dev) - Offscreen rendering for tests (enabled on `rustkit-engine` for hiwave-app's macOS dev-dependency)

On macOS, page content is always drawn by RustKit; the `webview-fallback`
feature (system WebKit for content) was removed in threads plan step T3.
On Windows and Linux the content WebView is still WRY.

## Test Fixtures

- `fixtures/` - HTML test files for rendering tests
- `websuite/` - Web compatibility test cases (micro tests, full pages)
- `baselines/` - Chrome reference screenshots
- `parity-baseline/` - RustKit vs Chrome comparison results
- `goldens/` - Golden images for visual regression

## Pull requests labelled `no-check`

A pull request carrying the GitHub label `no-check` is a standing or parked PR
(for example a long-lived ledger). Do not review it, stamp it, run a receipt on
it, push to it, or merge it, and leave it out of review queues and status
lists. Check a PR's labels before starting work on it. Removing the label puts
it back in the queue. Only the project lead adds or removes `no-check`; a worker
does not park its own PR. A `no-check` PR's `r2-pass` label or earlier stamp is
not a pass for its current head.

## Commit Message Format

Use conventional commits:
- `feat:` - New feature
- `fix:` - Bug fix
- `refactor:` - Code restructuring
- `test:` - Adding tests
- `docs:` - Documentation
- `chore:` - Maintenance

## IPC Communication

Chrome/Shelf WebViews communicate with Rust via `window.ipc.postMessage()`. The Rust backend handles messages in `ipc/commands.rs` and sends responses via `evaluate_script()`.

## Current Status and Plans

Engine status (parity, WPT, known gaps) lives in `README.md` under "Engine
Status". Current plans and design notes are in `docs/` (for example
`docs/PARITY_FINISH_LINE_PLAN_2026-08-04.md` and `docs/HIWAVE_MCP_PLAN.md`);
`trench/` holds run baselines and forensics. Superseded plans are kept in
`docs/archive/` for history only; do not treat them as current.

### Key Files for Parity Work

| File | What it does |
|------|--------------|
| `crates/rustkit-css/src/lib.rs` | CSS parsing, Display enum, color parsing |
| `crates/rustkit-engine/src/lib.rs` | Selector matching, style cascade |
| `crates/rustkit-layout/src/lib.rs` | Block/flex/inline-block layout |
| `crates/rustkit-renderer/src/lib.rs` | GPU rendering, gradients, backgrounds |
| `scripts/parity_test.py` | Run parity tests (`--scope micro` for CSS micro-tests) |
| `parity-baseline/diffs/` | Visual diff results and attribution JSON (generated, gitignored) |
