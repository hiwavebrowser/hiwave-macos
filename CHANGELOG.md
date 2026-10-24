# Changelog

Changes are tracked through merged pull requests into `develop`:
<https://github.com/hiwavebrowser/hiwave-macos/pulls?q=is%3Apr+is%3Amerged>

Releases are cut by promoting `develop` to `master` and tagging the result. The
full notes for each release, with measurements and known issues, are on the
[GitHub releases page](https://github.com/hiwavebrowser/hiwave-macos/releases).
This file only summarizes them.

## [Unreleased]

Everything merged into `develop` since V1.4.0. See the merged pull requests
linked above.

## [V1.4.0] - 2026-10-24

develop promoted to master on Friday 2026-10-23 ET.

- Pages run on their own content thread (on by default): the window no longer
  waits on page script; the address bar and Stop answer while a page loads.
- HiWave Inspector: Elements and Console panels for RustKit pages, JavaScript
  error stacks, shape-only privacy.
- CSS `:has()`, `@supports`, `color-mix()`, `-webkit-line-clamp`, scoped
  shadow DOM styles; clicks reach transformed consent dialogs.
- Quiet real-site board 28/60 (V1.3.0: 27/60); campaign 26/26 at 1.1%.

## [V1.3.0] - 2026-10-12

First weekly release (develop `16f97285` promoted by #660).

- Pages scroll; Wikipedia articles render with tables and floats; github.com
  settles instead of rebuilding forever; a runaway script is stopped after 15
  seconds (was 60).
- Layout: first slice of table layout, float line wrapping, auto margins on
  flex items. SVG sprite sheets and `<use>`. `opacity` honoured as a group layer.
- Quiet real-site board 27/60 (unchanged from V1.2.0).

## [V1.2.0] - 2026-10-02

develop promoted to master at `157767f` (#439).

- JavaScript: DOM read, mutate, events and forms, plus common web globals.
- CSS: cascade layers, nesting, `light-dark()`, elliptical corners, grid
  template areas, generic font families and glyph fallback.
- Web fonts painted with their own glyphs; rustls with HTTP/2; EasyList shield.
- Builtins 26/26 at mean diff 1.107% vs Chrome 148; real-site board 27/60.

## [V1.1.0] - 2026-09-24

develop `6e508fd` promoted to master (#248), 53 merged PRs.

- Text and layout: kerning, Blink baseline seating, justify, multi-column,
  grid and flex sizing fixes.
- CSS: `!important` and `inherit`, radial-gradient sizes, rounded and
  dashed/dotted borders.
- `parity-capture --url` and the real-site board; stop, reload, back and
  forward over session history.
- Parity campaign 26/26 at 1.2566 average diff; WPT Tier-1 23/26.

## [V1.0.001] - 2025-12-31

Updated about page.

## [V1.0.0-alpha] - 2025-12-31

First public release.

[Unreleased]: https://github.com/hiwavebrowser/hiwave-macos/compare/V1.3.0...develop
[V1.3.0]: https://github.com/hiwavebrowser/hiwave-macos/releases/tag/V1.3.0
[V1.2.0]: https://github.com/hiwavebrowser/hiwave-macos/releases/tag/V1.2.0
[V1.1.0]: https://github.com/hiwavebrowser/hiwave-macos/releases/tag/V1.1.0
[V1.0.001]: https://github.com/hiwavebrowser/hiwave-macos/releases/tag/V1.0.001
[V1.0.0-alpha]: https://github.com/hiwavebrowser/hiwave-macos/releases/tag/V1.0.0-alpha
