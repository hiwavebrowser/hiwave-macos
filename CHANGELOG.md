# Changelog

All notable changes to HiWave will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- About page with donation links
- Contributing guidelines
- GitHub Actions CI/CD workflow
- Comprehensive documentation

### Changed
- Improved README for public release

## [0.1.0-alpha] - 2024-XX-XX

### Added
- **Core Browsing**
  - Tab management (create, close, switch, reorder)
  - Address bar with URL/search detection
  - Navigation (back, forward, reload)
  - Keyboard shortcuts for all actions

- **The Shelf**
  - Drag-to-shelve tabs
  - Visual decay indicators
  - Workspace-scoped shelf view
  - One-click restore

- **Workspaces**
  - Create and manage workspaces
  - Inline rename
  - Tab count per workspace
  - Quick switching via sidebar

- **Flow Shield**
  - Built-in ad and tracker blocking
  - Powered by Brave's adblock-rust
  - Block count display
  - Per-domain blocking

- **Flow Vault**
  - Encrypted password storage (AES-256-GCM)
  - Master password protection
  - Add/search/copy credentials

- **UI/UX**
  - Dark theme
  - Sidebar with workspaces, shelf, and actions
  - Command palette (Ctrl+K)
  - Three mode selector (Essentials, Balanced, Zen)
  - New tab page with keyboard reference

### Known Limitations
- No bookmarks (planned)
- No history (planned)
- No Find in Page (planned)
- No context menus (planned)
- Vault auto-fill not implemented
- Mode automation not fully implemented

---

## Version History

| Version | Date | Highlights |
|---------|------|------------|
| 0.1.0-alpha | TBD | Initial alpha release |

---

## Roadmap Preview

### 0.2.0 (Planned)
- Find in Page (Ctrl+F)
- Context menus
- Basic bookmarks
- Session restoration

### 0.3.0 (Planned)
- History
- Downloads manager
- Import from Chrome/Firefox
- Tab audio indicators

### 1.0.0 (Future)
- Feature complete
- Stable for daily use
- All major browsers import support
