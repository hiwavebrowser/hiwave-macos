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

Found a bug? Please [open an issue](https://github.com/petecopeland/hiwave/issues/new) with:
- Steps to reproduce
- Expected behavior
- Actual behavior
- Your OS and version
- Screenshots if applicable

### 💡 Suggest Features

Have an idea? [Start a discussion](https://github.com/petecopeland/hiwave/discussions/new) or open an issue tagged with `enhancement`.

### 📖 Improve Documentation

Documentation improvements are always welcome! This includes:
- Fixing typos
- Clarifying instructions
- Adding examples
- Translating content

### 🧪 Test on Your Platform

Try HiWave on your system and report any issues. We especially need testers on:
- Windows (various versions)
- Linux (different distros)
- Older macOS versions

### 🔧 Write Code

Ready to dive in? See the [Development Setup](#development-setup) section below.

---

## Development Setup

### Prerequisites

See [INSTALL.md](INSTALL.md) for complete setup instructions. Quick summary:

| Platform | Requirements |
|----------|--------------|
| macOS | Xcode Command Line Tools |
| Windows | Visual Studio Build Tools with C++ |
| Linux | build-essential, libgtk-3-dev, libwebkit2gtk-4.1-dev |

Plus Rust 1.75+ on all platforms.

### Clone and Build

```bash
# Clone the repo
git clone https://github.com/petecopeland/hiwave.git
cd hiwave

# Build in debug mode (faster compilation)
cargo build -p hiwave-app

# Run
cargo run -p hiwave-app
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
hiwave/
├── crates/
│   ├── hiwave-app/        # Main application
│   │   ├── src/
│   │   │   ├── main.rs         # Entry point, event loop
│   │   │   ├── state.rs        # AppState, persistence
│   │   │   ├── ipc/            # IPC message handling
│   │   │   │   ├── mod.rs         # Message types
│   │   │   │   └── commands.rs    # Command handlers
│   │   │   ├── import/         # Browser import
│   │   │   ├── platform/       # OS-specific code
│   │   │   └── ui/             # HTML/CSS/JS for browser UI
│   │   │       ├── chrome.html    # Main UI
│   │   │       ├── shelf.html     # Command palette
│   │   │       ├── vault.html     # Password manager
│   │   │       └── new_tab.html   # New tab page
│   │   └── Cargo.toml
│   │
│   ├── hiwave-core/       # Shared types (TabId, WorkspaceId, etc.)
│   ├── hiwave-shell/      # Tab/workspace management, command palette
│   ├── hiwave-shield/     # Ad blocking engine
│   └── hiwave-vault/      # Password manager
│
├── Planning/               # Design documents (not code)
├── docs/                   # Technical documentation
├── INSTALL.md              # Installation guide
├── CONTRIBUTING.md         # This file
└── CLAUDE.md               # AI assistant context
```

### Key Files for New Contributors

| File | What it does |
|------|--------------|
| `hiwave-app/src/main.rs` | Application entry, event loop, WebView setup |
| `hiwave-app/src/state.rs` | AppState struct, persistence, shelf logic |
| `hiwave-app/src/ipc/mod.rs` | IPC message definitions |
| `hiwave-app/src/ipc/commands.rs` | Handlers for each IPC command |
| `hiwave-app/src/ui/chrome.html` | Main browser UI (HTML/CSS/JS) |
| `hiwave-shell/src/lib.rs` | Tab and workspace management |
| `hiwave-shield/src/lib.rs` | Ad blocking logic |

---

## Making Changes

### 1. Create a Branch

```bash
# Update main
git checkout main
git pull origin main

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

# Run tests
cargo test --workspace

# Check formatting
cargo fmt --all -- --check

# Run linter
cargo clippy --workspace -- -D warnings

# Actually run the app and test manually
cargo run -p hiwave-app
```

### 4. Commit

Write clear commit messages:

```
feat: add context menu for tabs

- Right-click on tab shows context menu
- Options: Close, Lock, Move to Workspace
- Closes #123
```

Prefixes:
- `feat:` - New feature
- `fix:` - Bug fix
- `docs:` - Documentation only
- `style:` - Formatting, no code change
- `refactor:` - Code change that neither fixes a bug nor adds a feature
- `test:` - Adding tests
- `chore:` - Maintenance tasks

---

## Pull Request Process

### 1. Open a Pull Request

- Go to [GitHub Pull Requests](https://github.com/petecopeland/hiwave/pulls)
- Click "New Pull Request"
- Select your branch
- Fill in the template

### 2. PR Template

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
- Once approved, your PR will be merged

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
cargo test -- --nocapture
```

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
| User-facing guides | `INSTALL.md`, `README.md` |
| Technical architecture | `docs/` |
| API documentation | Doc comments in code |
| Design decisions | `Planning/` |
| AI context | `CLAUDE.md` |

### Generating Docs

```bash
cargo doc --workspace --open
```

---

## Questions?

- Open a [GitHub Discussion](https://github.com/petecopeland/hiwave/discussions)
- Check existing [Issues](https://github.com/petecopeland/hiwave/issues)

Thank you for contributing to HiWave! 🎉
