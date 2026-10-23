<h1 align="center">HiWave</h1>

<p align="center">
  <strong>Focus. Flow. Freedom.</strong><br>
  A privacy-first browser that helps you close tabs, not open more.
</p>

<p align="center">
  <a href="#features">Features</a> •
  <a href="#download">Download</a> •
  <a href="#philosophy">Philosophy</a> •
  <a href="#contributing">Contributing</a> •
  <a href="#support">Support</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/status-alpha-blueviolet" alt="Status: Alpha" />
  <img src="https://img.shields.io/badge/license-MPL--2.0-blue" alt="License: MPL-2.0" />
  <img src="https://img.shields.io/badge/platform-macOS-lightgrey" alt="Platform: macOS" />
</p>

---

> **Mission:** a free, open-source browser with its own Rust engine, outside Google's ecosystem, that is as good as Chrome and better at privacy, security and user control. Read [MISSION.md](MISSION.md).

## The Problem

Modern browsers are designed to keep you browsing. More tabs, more tracking, more data vultures, more history, more extensions, more complexity. The result? Dozens of open tabs you'll "get to eventually," fractured attention, and digital clutter that drains your focus and steals your privacy.

## The Solution

**HiWave** flips the script. We built a browser that actively helps you browse *less* — in a good way.

- **The Shelf** — Tabs you're not using decay and fade away, so you don't have to manually manage them
- **Workspaces** — Separate contexts (work, personal, research) that don't bleed into each other
- **Built-in Privacy** — Ad and tracker blocking with no extensions needed
- **Three Modes** — Choose your level of automation: do it yourself, get suggestions, or let Zen handle it

---

## Engine Status — macOS

macOS is the **reference tree** for RustKit: the Windows and Linux ports are
measured against this tree's behaviour. It is also currently the only platform
with pixel-level parity capture against Chrome.

| | |
|---|---|
| Build | **passing**. CI compiles every workspace test (`cargo test --workspace --no-run`) and runs each crate's unit suite on every PR. |
| Real-site parity | **19 / 60** on the top-20 live-site board against pinned Chrome for Testing 148, up from 10 a week earlier (`scripts/realsite_board.py`, `trench/realsite/trend.csv` on the `atlas/trench-realsite` branch, 2026-09-26) |
| Fixed-page parity | **26 / 26 cases, 1.25% average diff** from Chrome, down from 2.48% on 2026-09-18 (`scripts/parity_test.py`, 2026-09-26) |
| WPT Tier-1 conformance | 6 / 12 scored at last run (2026-07-31); see `trench/wpt/last-run.json` |

The fixed-page campaign is saturated: every case passes. It guards against
regressions. **The real-site board is the number to watch:** it loads live
sites and scores each one on *loads*, *readable* and *looks right* against the
same Chrome.

### What landed recently

See the [merged pull requests](https://github.com/hiwavebrowser/hiwave-macos/pulls?q=is%3Apr+is%3Amerged+base%3Adevelop)
into `develop`.

### Known gaps — stated, not hidden

- **JavaScript is partial.** Page scripts run in Boa with a limited set of DOM
  APIs, so JS-rendered sites (youtube, instagram, bing) show their server HTML
  only. Custom elements and Shadow DOM aren't implemented yet.
- **No live-site gate in CI.** Unit suites gate every PR, but the real-site
  board runs on a dedicated Mac every 3 hours, not in CI.
- **Animations are parsed, not executed** — transition/animation properties
  compute and survive the cascade; nothing ticks yet
- **Two WPT cases render blank** (`empty-span-scroll`,
  `align-items-baseline-overflow-non-visible`) — recorded as render refusals
  rather than scored, because two blank frames "match" each other for the
  worst possible reason
- **Text metrics** remain the largest single source of parity diff vs Chrome
- **No build/tests feed to the umbrella yet** — this repo's `metrics-history`
  branch carries parity data only, so the umbrella's macOS *build* badge
  honestly reads "unknown" until we publish the Windows/Linux-shaped feed

### How these numbers are produced

**Tests**: CI compiles the whole workspace's tests and runs every crate's unit
suite on each PR (`.github/workflows/parity.yml`).

**Parity** — `scripts/parity_swarm.py` against Chrome baselines, published per
master commit to
[`metrics-history`](https://github.com/hiwavebrowser/hiwave-macos/tree/metrics-history)
(`metrics/history.csv`, append-only) and aggregated by the
[umbrella repo](https://github.com/hiwavebrowser/hiwave)'s `metrics.yml`,
which renders the badges. The figure above is the last `master` row of that
CSV, not the committed `parity-baseline/` snapshot — those diverge, and
reading the snapshot is how an earlier version of this section reported a
three-week-old 88.1% / 21-of-26 while the live feed already said 93.4% /
26-of-26.

**WPT** — `scripts/wpt_tier1.py`, engine-vs-engine reftests at the manifest
pin; the rate excludes skips and errors from its denominator, and the harness
refuses to publish unless a deliberately-mismatched control fails first.

A number with no path back to a machine that measured it does not appear on
this page — and a number whose path leads to a stale file is the same defect
wearing better clothes.

---

## Features

### 🗂️ The Shelf
Park tabs for later without leaving them open. Shelved items show their age, naturally fading so forgotten pages don't haunt you forever.

### ⏰ Tab Decay
Unused tabs gradually fade, giving you visual cues about what's actually important. In Zen mode, old tabs automatically shelve themselves.

### 🛡️ Flow Shield
Native ad and tracker blocking powered by Brave's engine. No extension required. Just fast, private browsing out of the box.

### 🔐 Flow Vault
Built-in password manager with AES-256 encryption. Your credentials stay local and secure.

### 🗃️ Workspaces
Separate your browsing contexts completely. Work tabs stay in Work, personal stays in Personal. Switch instantly with keyboard shortcuts.

### ⌨️ Keyboard First
Power users rejoice. Everything is accessible via keyboard:
- `Ctrl+K` — Command palette (search anything)
- `Ctrl+Shift+S` — Shelve current tab
- `Ctrl+B` — Toggle sidebar
- `Ctrl+1-9` — Jump to specific tab

### 🎛️ Three Modes
| Mode | For | What It Does |
|------|-----|--------------|
| **Essentials** | Control freaks | Manual everything |
| **Balanced** | Most people | Smart suggestions |
| **Zen** | Trust the system | Full automation |

---
> **Note:** HiWave is currently in alpha. Expect some rough edges!

### Build from Source

macOS is the supported platform for this repository: RustKit content
rendering, the `rustkit` feature's engine dependencies and every CI job that
builds Rust are macOS-only (`macos-14`). The Rust toolchain is pinned by
`rust-toolchain.toml` (currently 1.91.0); `rustup` picks it up automatically.

```bash
git clone https://github.com/hiwavebrowser/hiwave-macos.git
cd hiwave-macos
cargo run -p hiwave-app            # debug build; the binary is `hiwave`
cargo run -p hiwave-app --release  # optimized build
./scripts/run-rustkit.sh           # same, via the convenience script
```

On macOS, page content is always rendered by RustKit (GPU rendering via wgpu,
engine-level ad/tracker blocking through `hiwave-shield`). The browser UI is
still drawn by the system WebView through WRY.

**Feature flags** (`hiwave-app`):

| Feature | Default | What it does |
|---------|---------|--------------|
| `rustkit` | yes | Pulls in the RustKit engine crates and wgpu for content rendering (macOS targets only) |

`rustkit-engine` also has a `headless` feature (offscreen rendering, no window
surface), which `hiwave-app` enables for its macOS integration tests. The old
`webview-fallback` mode has been removed.

### Tests

```bash
cargo test --workspace --no-run      # CI: every test target must compile
cargo test -p rustkit-layout --lib   # CI runs the lib suites of layout, engine, js,
                                     # bindings, renderer, image, viewhost and http
cargo test -p rustkit-js --tests     # JS engine integration tests
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings

# Python instrument guards (CI runs these on Ubuntu; needs pyyaml)
for t in scripts/tests/test_*.py; do python3 "$t"; done
```

### Parity against Chrome

```bash
cargo build --release -p parity-capture
python3 scripts/parity_test.py                    # all campaign cases
python3 scripts/parity_test.py --scope builtins   # builtins | websuite | micro | holdout
python3 scripts/parity_test.py --case new_tab     # one case
python3 scripts/parity_swarm.py --jobs 4 --scope all
python3 scripts/generate_baselines.py             # regenerate Chrome baselines
```

Chrome baselines need the oracle's Node dependencies (`npm ci` in
`tools/parity_oracle`). Test cases are registered in `cases/registry.json`.

### Workspace

| Area | Crates |
|------|--------|
| App | `hiwave-app` (entry point, event loop, IPC, UI in `src/ui/`), `hiwave-core`, `hiwave-shell` (tabs, workspaces, command palette), `hiwave-shield` (adblock-rust), `hiwave-vault` (AES-256 passwords), `hiwave-analytics` (local-only) |
| Engine core | `rustkit-engine`, `rustkit-core`, `rustkit-common`, `rustkit-dom`, `rustkit-html`, `rustkit-css`, `rustkit-cssparser`, `rustkit-layout`, `rustkit-text` |
| Rendering | `rustkit-renderer` (wgpu), `rustkit-compositor`, `rustkit-viewhost`, `rustkit-image`, `rustkit-codecs`, `rustkit-svg`, `rustkit-canvas`, `rustkit-webgl` |
| Scripting and web APIs | `rustkit-js` (Boa, vendored with a patch in `third_party/boa_engine`), `rustkit-bindings`, `rustkit-animation`, `rustkit-worker`, `rustkit-sw`, `rustkit-idb`, `rustkit-media`, `rustkit-a11y` |
| Network | `rustkit-net`, `rustkit-http` |
| Testing and tools | `rustkit-test`, `rustkit-bench`, `parity-capture`, `hiwave-mcp`, `hiwave-smoke` |

Docs and plans are in [`docs/`](docs/); superseded plans are in
[`docs/archive/`](docs/archive/).

---

## Philosophy

### Attention over Tabs
We don't measure success by how many tabs you open. We measure it by how focused you stay.

### Simplicity over Extensibility  
No extension ecosystem, by design. Plugins are a common path for tracking and malware, and a slower browser. What people use extensions for, like ad blocking and password management, is built in, tested and integrated. One browser, one experience.

### Privacy by Default
Tracking protection isn't an add-on, it's foundational. We don't collect your data. Period.

### Modern Web Only
We target post-2020 web standards. No legacy cruft in general. The exception is the popular sites on our real-site boards: we support those as they really are, old hacks included, because people use them every day.

### Opinionated but Respectful
We have strong opinions about how browsing should work, but we offer three modes so you can choose your level of buy-in.

---

## Screenshots

<p align="center">
  <em>Coming soon — the UI is still evolving!</em>
</p>

---

## Roadmap

### Now (Alpha)
- ✅ Core browsing (tabs, navigation, address bar)
- ✅ The Shelf with decay visualization
- ✅ Workspaces
- ✅ Flow Shield (ad blocking)
- ✅ Flow Vault (password manager)
- ✅ Command palette
- ✅ Settings page
- 🔄 Bidirectional IPC 
- ✅ Find in Page (Ctrl+F)
- ✅ History
- ✅ Downloads manager
- ✅ Context menus
- ✅ Import from Chrome/Firefox
- ✅ Tab audio indicators

### Future
- [ ] Workspace Sync (cross-device)
- [ ] Reader Mode
- [ ] Themes (light mode)
- [ ] Mobile companion

---

## Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for:
- Development setup
- Code style guidelines
- Pull request process
- Areas where we need help

**Quick Start:**
```bash
cargo test --workspace                  # Run tests
cargo fmt --all                         # Format code
cargo clippy --workspace -- -D warnings # Lint
```

---

## Support HiWave's Development

HiWave is **free and open source**. No ads, no tracking, no data selling.

If HiWave helps you focus better, consider supporting its development:

<p align="center">
  <a href="https://github.com/sponsors/hiwavebrowser">
    <img src="https://img.shields.io/badge/sponsor-GitHub%20Sponsors-ea4aaa" alt="GitHub Sponsors" />
  </a>
  <a href="https://ko-fi.com/hiwavebrowser">
    <img src="https://img.shields.io/badge/support-Ko--fi-ff5e5b" alt="Ko-fi" />
  </a>
</p>

Your support helps cover:
- Development time
- Infrastructure costs
- Future features like Workspace Sync

---

## Architecture

Web pages are rendered by **RustKit**, HiWave's own engine, which is the
default. The browser's own UI (tabs, address bar, sidebar, and the shelf
panel below the page) is still HTML rendered by the system WebView through
WRY, and talks to Rust over IPC (`window.ipc.postMessage()`, handled in
`crates/hiwave-app/src/ipc/commands.rs`). Moving that UI onto RustKit too,
so that nothing borrowed remains, is part of the [mission](MISSION.md).

```
┌─────────────────────────────────────────┐
│  Chrome WebView (Browser UI, WRY)       │
│  Tabs • Address Bar • Sidebar           │
├─────────────────────────────────────────┤
│                                         │
│  Content (Web Pages): RustKit engine    │
│                                         │
├─────────────────────────────────────────┤
│  Shelf WebView (WRY)                    │
│  Shelf • Command palette                │
└─────────────────────────────────────────┘
```

Built with:
- **Rust**: core logic and memory safety
- **RustKit**: our own engine (HTML, CSS, layout, GPU rendering via wgpu, and JS via Boa)
- **WRY/Tao**: windowing, and the browser UI's WebView for now
- **Brave's adblock-rust** — Ad blocking engine
- **Vanilla JS** — No framework bloat in the UI

---

## License

HiWave is licensed under the [Mozilla Public License 2.0](LICENSE).

This means:
- ✅ Free to use, modify, and distribute
- ✅ Source code is open
- ✅ You can build commercial products with it
- ⚠️ Changes to HiWave's files must be shared under MPL-2.0
 
For commercial licensing options, see COMMERCIAL-LICENSE.md.

---

## FAQ

**Q: Why not just use Firefox/Brave/Arc?**  
A: They're great browsers! Brave and Arc are built on Chromium, Google's engine. HiWave has its own engine, written from scratch in Rust, so it sits outside Google's ecosystem. None of them have The Shelf, tab decay, or our focus on reducing cognitive load.

**Q: Is this production-ready?**  
A: Not yet. We're in alpha. Use it as a secondary browser while we iron out the kinks.

**Q: Will there be a mobile version?**  
A: Eventually! Desktop is the priority for now.

**Q: How do you make money?**  
A: We don't yet. A possible future option is a paid, optional Workspace Sync. No search partnerships are planned. We will never sell your data or show ads.

---

<p align="center">
  <strong>Built with 💜 for people who want to focus.</strong>
</p>

<p align="center">
  <a href="https://www.hiwavebrowser.com">Website</a> •
  <a href="https://github.com/hiwavebrowser/hiwave-macos">GitHub</a> •
  <a href="https://twitter.com/hiwavebrowser">Twitter</a>
</p>
