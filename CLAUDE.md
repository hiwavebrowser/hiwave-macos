# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

HiWave is a privacy-first browser for macOS built in Rust. It features a custom browser engine called **RustKit** that provides pure-Rust rendering with GPU acceleration via wgpu, alongside native ad/tracker blocking via Brave's adblock-rust engine.

## Build Commands

```bash
# Build and run (default RustKit mode)
cargo run -p hiwave-app --features rustkit

# Release build
cargo run -p hiwave-app --features rustkit --release

# WebKit fallback mode (uses system WebKit for all rendering)
cargo run -p hiwave-app --no-default-features --features webview-fallback

# Quick syntax check
cargo check -p hiwave-app

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
- **Content WebView** (RustKit or WebKit): Web page rendering
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
- `rustkit-engine` - Engine orchestration, multi-view management
- `rustkit-dom` - DOM tree representation
- `rustkit-html` - HTML parser
- `rustkit-css` / `rustkit-cssparser` - CSS parsing and styling
- `rustkit-layout` - Flexbox/block layout
- `rustkit-renderer` - GPU rendering via wgpu
- `rustkit-compositor` - Layer compositing
- `rustkit-js` / `rustkit-bindings` - JavaScript engine integration
- `rustkit-net` / `rustkit-http` - Network stack
- `rustkit-image` / `rustkit-svg` - Image/SVG handling
- `rustkit-text` - Text shaping and rendering
- `rustkit-viewhost` - Window/surface management

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
- `webview-fallback` - Use system WebKit for everything
- `headless` (dev) - Offscreen rendering for tests

## Test Fixtures

- `fixtures/` - HTML test files for rendering tests
- `websuite/` - Web compatibility test cases (micro tests, full pages)
- `baselines/` - Chrome reference screenshots
- `parity-baseline/` - RustKit vs Chrome comparison results
- `goldens/` - Golden images for visual regression

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

## Recent Work (January 2026)

### Parity Improvement Progress

Working through `PARITY_IMPROVEMENT_PLAN.md` to improve RustKit visual parity with Chrome.

**Completed:**
- Phase 1: Background layer foundation (HSL/HSLA colors, gamma-correct gradient interpolation)
- Phase 2: Selector engine fixes (combinator matching, ancestor traversal)
- Inline-block layout implementation in `rustkit-layout`
- Repeating gradient support (linear, radial, conic) in `rustkit-renderer`

**Key commits:**
- `6a5624c` feat: Add inline-block layout and repeating gradient support (branch: `feat/inline-block-and-repeating-gradients`)
- `8fb758f` feat: Add HSL/HSLA color parsing to engine
- `08d996b` fix: Prevent GPU buffer overflow in gradient rendering

**Parity test results (micro scope):**
- backgrounds: 40.2% → 20.6% (improved)
- combinators: 16.2% → 15.4% (improved)
- css-selectors: 32.2% → 29.7% (improved)

**Remaining issues:**
- Text metrics account for ~59% of remaining diffs (font rendering differences)
- Gradient interpolation ~16% (color space handling)

**Next phases from plan:**
- Phase 3: Text rendering alignment
- Phase 4: Form controls
- Phase 5: Advanced layout (grid, sticky)
- Phase 6: Images and replaced elements

### Key Files for Parity Work

| File | What it does |
|------|--------------|
| `crates/rustkit-css/src/lib.rs` | CSS parsing, Display enum, color parsing |
| `crates/rustkit-engine/src/lib.rs` | Selector matching, style cascade |
| `crates/rustkit-layout/src/lib.rs` | Block/flex/inline-block layout |
| `crates/rustkit-renderer/src/lib.rs` | GPU rendering, gradients, backgrounds |
| `scripts/parity_test.py` | Run parity tests (`--scope micro` for CSS micro-tests) |
| `parity-baseline/diffs/` | Visual diff results and attribution JSON |
