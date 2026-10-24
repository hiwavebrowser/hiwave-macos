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
| Release | **V1.4.0** (2026-10-24), the weekly promotion of `develop` to `master` |
| Build | **passing**. CI compiles every workspace test (`cargo test --workspace --no-run`) and runs each crate's unit suite on every PR; develop runs a post-merge compile check. |
| Real-site parity | **28 / 60** on the top-20 live-site board against pinned Chrome for Testing 148, **33 / 78** on the 26-site list (quiet board at develop `90d38b2b`, 2026-10-22; `scripts/realsite_board.py`) |
| Fixed-page parity | **26 / 26 cases, 1.1% average diff** from Chrome (`scripts/parity_test.py`, every receipt on 2026-10-23) |
| WPT Tier-1 conformance | 6 / 12 scored at last run (2026-07-31); see `trench/wpt/last-run.json` |

The fixed-page campaign is saturated: every case passes. It guards against
regressions. **The real-site board is the number to watch:** it loads live
sites and scores each one on *loads*, *readable* and *looks right* against the
same Chrome.

### Where macOS is (V1.4.0)

- **Pages run on their own thread.** Page script, style, layout and loading
  run on a content thread (on by default; `HIWAVE_CONTENT_THREAD=0` turns it
  off). With it on, the window never stalled waiting on the page in 5 of 5
  loads each of yahoo, cnn and lyft, and the address bar answers in about
  50 ms while a page loads. Stop works in under 100 ms.
- **HiWave Inspector.** An Elements and Console panel for RustKit pages, with
  JavaScript error stacks (frames, script URL, line, column, and which script
  inserted the failing one). It shows only the *shape* of page data
  (`<string len=13>`, `<text len=N>`, URLs without query or fragment), never
  the page's own text.
- **CSS:** `:has()`, `@supports` conditions, `color-mix()`,
  `-webkit-line-clamp`, shadow DOM styles (`:host`, `::slotted`, `:defined`),
  `font-size: 0`, `box-sizing: inherit`, percentage `auto-fit` tracks, and an
  invalid shorthand is dropped whole, as Chrome does.
- **Input:** clicks reach transformed and nested-fixed consent dialogs
  (OneTrust-style banners); typing in the address bar replaces the URL;
  positioned form fields are drawn and clicked where they belong.
- **Loading:** `window load` waits for scripts the page inserted while
  loading, as the HTML standard says (cnn now shows its consent dialog, as
  Chrome does); a 204 response leaves the page as it was; images decode on a
  two-thread pool; `data:` fonts with a `#fragment` load.

### What landed recently

See the [merged pull requests](https://github.com/hiwavebrowser/hiwave-macos/pulls?q=is%3Apr+is%3Amerged+base%3Adevelop)
into `develop`, and [CHANGELOG.md](CHANGELOG.md).

### Known gaps — stated, not hidden

- **JavaScript is partial.** Page scripts run in Boa with a growing but
  incomplete set of web APIs. Heavy JS-rendered sites (youtube, instagram)
  still show little more than their server HTML; yahoo's below-the-fold feed
  stays empty.
- **Layout is not re-run when a script measures what it just changed** inside
  one task, so "hide until it fits" loops can run long. This blocks
  `display: contents`, which is still dropped.
- **Not yet supported:** `@container` queries, `clip-path`, `filter`, CSS
  `fill`/`stroke` on inline SVG, list markers, 3D transforms (all in review or
  queued).
- **The browser UI is still drawn by the system WebView (WRY).** Moving it
  onto RustKit is the next major milestone.
- **No live-site gate in CI.** Unit suites gate every PR; the real-site board
  runs on a dedicated Mac.
- **Animations are parsed, not executed.**
- **Text metrics** remain the largest single source of parity diff vs Chrome.

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
