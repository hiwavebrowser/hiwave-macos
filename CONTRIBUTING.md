# Contributing to HiWave

Thank you for your interest in contributing to HiWave! This document provides guidelines and information for contributors.

---

## Table of Contents

1. [Code of Conduct](#code-of-conduct)
2. [Ways to Contribute](#ways-to-contribute)
3. [Development Setup](#development-setup)
4. [Project Structure](#project-structure)
5. [Making Changes](#making-changes)
6. [Pull Request Process](#pull-request-process)
7. [Coding Standards](#coding-standards)
8. [Testing](#testing)
9. [Documentation](#documentation)

---

## Code of Conduct

Be kind. Be respectful. Assume good intentions. We're all here to build something great together.

---

## Ways to Contribute

### 🐛 Report Bugs

Found a bug? Please [open an issue](https://github.com/hiwavebrowser/hiwave-macos/issues/new) with:
- Steps to reproduce
- Expected behavior
- Actual behavior
- Your macOS version
- Screenshots if applicable

### 💡 Suggest Features

Have an idea? Open an [issue](https://github.com/hiwavebrowser/hiwave-macos/issues/new) tagged with `enhancement`.

### 📖 Improve Documentation

Documentation improvements are always welcome! This includes:
- Fixing typos
- Clarifying instructions
- Adding examples
- Translating content

### 🧪 Test on Your Machine

Try HiWave on your Mac and report any issues, especially sites that do not
render the way Chrome does. Older macOS versions and Intel Macs are useful to
hear about.

### 🔧 Write Code

Ready to dive in? See the [Development Setup](#development-setup) section below.

---

## Development Setup

### Prerequisites

macOS is the supported platform for this repository: RustKit content
rendering and every CI job that builds Rust are macOS-only.

| Requirement | Notes |
|-------------|-------|
| macOS with Xcode Command Line Tools | `xcode-select --install` |
| Rust via `rustup` | The toolchain is pinned in `rust-toolchain.toml` (currently **1.91.0**); `rustup` installs and selects it automatically |
| Python 3 | For the scripts in `scripts/` and the instrument guards (`pyyaml` needed) |
| Node.js | Only for regenerating Chrome baselines (`npm ci` in `tools/parity_oracle`) |

On Linux, `cargo check -p hiwave-app` additionally needs the GTK and
WebKitGTK development packages (`libgtk-3-dev`, `libwebkit2gtk-4.1-dev`).

### Clone and Build

```bash
# Clone the repo
git clone https://github.com/hiwavebrowser/hiwave-macos.git
cd hiwave-macos

# Build and run (debug; the `rustkit` feature is on by default)
cargo run -p hiwave-app

# Optimized build
cargo run -p hiwave-app --release
```

### Useful Commands

```bash
# Fast syntax check (no binary output)
cargo check -p hiwave-app

# Run tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p hiwave-shield

# Format code
cargo fmt --all

# Run linter
cargo clippy --workspace -- -D warnings

# Generate documentation
cargo doc --workspace --open
```

---

## Project Structure

```
hiwave-macos/
├── crates/
│   ├── hiwave-app/        # Main application
│   │   ├── src/
│   │   │   ├── main.rs             # Entry point, event loop, WebView setup
│   │   │   ├── state.rs            # AppState, persistence, shelf logic
│   │   │   ├── ipc/                # IPC message types and command handlers
│   │   │   ├── webview_rustkit.rs  # RustKit content WebView integration
│   │   │   ├── shield_adapter.rs   # Ad blocking for RustKit requests
│   │   │   └── ui/                 # HTML/CSS/JS for browser UI
│   │   └── Cargo.toml
│   │
│   ├── hiwave-core/       # Shared types (TabId, WorkspaceId, etc.)
│   ├── hiwave-shell/      # Tab/workspace management, command palette
│   ├── hiwave-shield/     # Ad blocking engine (Brave's adblock-rust)
│   ├── hiwave-vault/      # Password manager
│   ├── hiwave-analytics/  # Local-only usage analytics
│   ├── rustkit-*/         # The RustKit engine (DOM, CSS, layout, text, renderer, JS, network, ...)
│   ├── parity-capture/    # Headless frame capture for parity testing
│   ├── hiwave-mcp/        # MCP server exposing the engine to agents
│   └── hiwave-smoke/      # Scripted smoke harness
│
├── scripts/                # Parity, capture and board scripts; scripts/tests/ holds the instrument guards
├── tools/parity_oracle/    # Chrome (Playwright) baseline capture
├── websuite/, fixtures/    # Web compatibility and rendering test pages
├── docs/                   # Plans and technical notes (superseded plans in docs/archive/)
├── CONTRIBUTING.md         # This file
└── CLAUDE.md               # AI assistant context
```

The full crate list is in the "Workspace" table in [README.md](README.md).

### Key Files for New Contributors

| File | What it does |
|------|--------------|
| `hiwave-app/src/main.rs` | Application entry, event loop, WebView setup |
| `hiwave-app/src/state.rs` | AppState struct, persistence, shelf logic |
| `hiwave-app/src/ipc/` | IPC message definitions and command handlers |
| `hiwave-app/src/ui/chrome.html` | Main browser UI (HTML/CSS/JS) |
| `hiwave-shell/src/lib.rs` | Tab and workspace management |
| `hiwave-shield/src/lib.rs` | Ad blocking logic |
| `rustkit-engine/src/lib.rs` | Engine orchestration, selector matching, style cascade |
| `rustkit-layout/src/lib.rs` | Block/inline/flex layout |
| `rustkit-renderer/src/lib.rs` | GPU rendering via wgpu |

---

## Making Changes

### 1. Create a Branch

Work branches off `develop`, which is also the target of pull requests.

```bash
# Update develop
git checkout develop
git pull origin develop

# Create a feature branch
git checkout -b feature/your-feature-name
# or
git checkout -b fix/your-bug-fix
```

### 2. Make Your Changes

- Keep changes focused and minimal
- Follow existing code style
- Add tests if applicable
- Update documentation if needed

### 3. Test Your Changes

```bash
# Make sure it compiles
cargo check -p hiwave-app

# What CI runs: compile every test target, then the per-crate suites
cargo test --workspace --no-run
cargo test -p rustkit-layout --lib   # likewise rustkit-engine, rustkit-js, rustkit-bindings,
                                     # rustkit-renderer, rustkit-image, rustkit-viewhost, rustkit-http
cargo test -p rustkit-js --tests     # JS engine integration tests

# Python instrument guards
for t in scripts/tests/test_*.py; do python3 "$t"; done

# Formatting and lint
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings

# Actually run the app and test manually
cargo run -p hiwave-app
```

If your change affects rendering, run the parity suite against Chrome too (see
[Testing](#testing)).

### 4. Commit

Use [conventional commits](https://www.conventionalcommits.org/):

```
feat: add context menu for tabs

- Right-click on tab shows context menu
- Options: Close, Lock, Move to Workspace
- Closes #123
```

Prefixes:
- `feat:` - New feature
- `fix:` - Bug fix
- `refactor:` - Code restructuring that neither fixes a bug nor adds a feature
- `test:` - Adding tests
- `docs:` - Documentation only
- `chore:` - Maintenance tasks

A scope is welcome when it helps, for example `fix(layout): ...` or
`feat(engine): ...`.

---

## Pull Request Process

### 1. Open a Pull Request

- Go to [GitHub Pull Requests](https://github.com/hiwavebrowser/hiwave-macos/pulls)
- Click "New Pull Request"
- Select your branch, with `develop` as the base
- Describe the change using the outline below

### 2. PR Description

```markdown
## Description
Brief description of what this PR does.

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## How Has This Been Tested?
Describe how you tested your changes.

## Checklist
- [ ] My code follows the project's style guidelines
- [ ] I have performed a self-review
- [ ] I have added tests (if applicable)
- [ ] I have updated documentation (if applicable)
- [ ] My changes generate no new warnings
```

### 3. Review Process

- A maintainer will review your PR
- Address any feedback
- Once approved and CI is green, your PR will be merged

### PRs labelled `no-check`

A pull request carrying the `no-check` label is a standing or parked PR (for
example a long-lived ledger). Do not review it, stamp it, run a receipt on it,
push to it, or merge it, and leave it out of review queues and status lists.
Check a PR's labels before starting work on it. Removing the label puts it
back in the queue. Only the project lead adds or removes `no-check`; a
contributor does not park their own PR. A `no-check` PR's `r2-pass` label or
earlier stamp is not a pass for its current head.

---

## Coding Standards

### Rust Style

- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Run `cargo fmt` before committing
- Run `cargo clippy` and fix warnings
- Document public APIs with doc comments

```rust
/// Creates a new tab with the given URL.
///
/// # Arguments
///
/// * `url` - The URL to load in the new tab
///
/// # Returns
///
/// The ID of the newly created tab
///
/// # Example
///
/// ```
/// let tab_id = shell.create_tab("https://example.com");
/// ```
pub fn create_tab(&mut self, url: &str) -> TabId {
    // ...
}
```

### JavaScript/HTML Style

- Use 2-space indentation
- Use `const` and `let`, never `var`
- Use template literals for string interpolation
- Document functions with JSDoc comments

```javascript
/**
 * Renders the tab list in the UI.
 * @param {Array<Tab>} tabs - Array of tab objects to render
 */
function renderTabs(tabs) {
    // ...
}
```

### CSS Style

- Use CSS custom properties for theming
- Follow BEM-ish naming: `.component-element--modifier`
- Keep selectors shallow

---

## Testing

### Running Tests

```bash
# All tests
cargo test --workspace

# Specific crate
cargo test -p hiwave-shield

# Specific test
cargo test test_should_block

# With output
cargo test test_name -- --nocapture
```

### Parity Against Chrome

RustKit rendering is compared against Chrome baselines pixel by pixel:

```bash
cargo build --release -p parity-capture
python3 scripts/parity_test.py                    # all campaign cases
python3 scripts/parity_test.py --scope builtins   # builtins | websuite | micro | holdout
python3 scripts/parity_test.py --case new_tab     # one case
python3 scripts/parity_swarm.py --jobs 4 --scope all
python3 scripts/generate_baselines.py             # regenerate Chrome baselines
```

Test cases are registered in `cases/registry.json`.

### Writing Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_works() {
        let result = some_function();
        assert_eq!(result, expected_value);
    }

    #[test]
    fn test_edge_case() {
        // Test edge cases and error conditions
    }
}
```

---

## Documentation

### Where to Document

| Type | Location |
|------|----------|
| User-facing guides and engine status | `README.md` |
| Plans and technical notes | `docs/` (superseded plans go to `docs/archive/`) |
| Run baselines and forensics | `trench/` |
| API documentation | Doc comments in code |
| AI context | `CLAUDE.md` |

### Generating Docs

```bash
cargo doc --workspace --open
```

---

## Questions?

- Check existing [Issues](https://github.com/hiwavebrowser/hiwave-macos/issues) or open a new one

Thank you for contributing to HiWave! 🎉
