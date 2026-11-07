# Real-window driver (macOS)

Runs checks against the **built HiWave app**, in its real window, with nobody
at the Mac. The headless engine tests and `parity-capture` do not go through
the app's event loop, its chrome WebViews or AppKit; this does.

```
python3 tools/real_window/driver.py --app <path to the hiwave binary> [--checks h1,h1_slow] [--out DIR]
python3 tools/real_window/driver.py --list
```

Build the app from the worktree you are testing and pass that binary. The
driver prints its path and sha256 and writes them to `result.json`.

The `a4_*` checks (the RustKit shelf, Inspector and settings) read the panel
probe, which is compiled into debug builds and into a release build only
with `cargo build --release -p hiwave-app --features test-hooks`; on a
binary without it they report `NOT RUN`. The probe reports shapes only (box,
match count, `checked`, value length), never text or values. The driver
starts the app with `HIWAVE_TEST_HOOKS=1`, which lets it run
`HIWAVE_RUSTKIT_CHROME_EVAL` and log chrome IPC bodies; a RustKit panel's IPC
is logged as its `cmd` and size only, always.

## Which chrome

The RustKit chrome (chrome.html, the shelf, the Inspector and the settings
page as RustKit views) is the app's default on macOS (CH3.A5a), so every
check runs on it unless it says otherwise; the app logs
`Chrome: RustKit (default; ...)` at startup. `HIWAVE_RUSTKIT_CHROME=0` keeps
the WebKit chrome (the escape hatch). Only the checks that cover the escape
hatch set it: `e9_redirect_webkit` and the flag-off half of
`a7_no_bridge_in_content`.

## What a run does

For each check it starts a fresh app on a throwaway profile, restored on one
page from `pages/` served by a local fixture server, and asserts on:

- **the fixture server's request log**: which requests the app made and when
  (a page reports what its scripts did by fetching `/beacon?<name>`);
- **the app's log**;
- **the window's pixels**: `screencapture -l <window id>`, then counts of the
  flat colours the pages are made of;
- after **real input**: `hwdrive` posts HID events (move, press, release,
  wheel, keys) at the window and resizes it through Accessibility.

The app's profile is isolated by setting `HOME` for the app process only (it
reads its data directory from `$HOME`), so the person's tabs, history and
vault are not read or written. The first launch lets the app write its own
`workspace_state.json`; the driver only swaps the tab URL in it. That
first-run profile is kept as `profile-seed/` in the output directory and
copied back before every check, so a check never starts on what an earlier
check saved (settings, the vault, the focus blocklist).

## Results

Every assertion ends as `PASS`, `FAIL` or `NOT RUN`. `NOT RUN` names what the
seat lacks and is never counted as a pass. Exit code: `0` all passed, `1` at
least one failed, `3` none failed but some did not run, `2` the driver could
not run. `--out` gets `result.json` (assertions and the full request log),
one `<check>.app.log` per check and the captured frames.

## What needs what

| part | needs |
|---|---|
| request-log and app-log assertions | nothing |
| frames | Screen Recording grant, screen unlocked |
| move, click, wheel, keys | Accessibility grant, screen unlocked |
| resize | Accessibility grant, screen unlocked |
| `hwdrive find` (elements of the chrome and settings WebViews by their accessible text or role) | Accessibility grant |

The grants belong to the program that starts the driver (the terminal, or
the program a launchd job runs), not to the script. `hwdrive request`, run by
hand at the Mac, makes macOS show its prompts and list that program in
System Settings > Privacy & Security. `hwdrive preflight` prints what the
seat has. With the screen locked, posted events go to the login window and a
window capture fails, so input and frames cannot run on a locked Mac whatever
the grants.

Input moves the real pointer and brings the app to the front for the length
of a run (about a minute for all checks).

## Checks

| check | hand-test item | page | needs input |
|---|---|---|---|
| `h1` | H1 late content | `h1_late.html`: a tick a second for 8 s, a fetch and an image started after the load | no |
| `h1_slow` | H1 | `h1_slow.html`: a 200 ms tick, then one request held for 3 s | no |
| `h2` | H2 resize | `h2_resize.html`: a 200x100 px box and a 50% box | resize |
| `h3` | H3 hover and press | `h3_hover.html`: one viewport-sized target with `:hover` and `:active` | yes |
| `h4` | H4 images | `h4_images.html`: five flat-colour images | no |
| `h4_slow` | H4 | `h4_slow.html`: a 200 ms tick, then one image held for 3 s | no |
| `h6` | H6 scroll | `h6_scroll.html`: three 1500 px bands | yes |
| `h6_extent` | H6 scroll | `h6_extent.html`: three 1500 px bands overflowing a `height: 100%` html, body and wrapper; script scrolls to the end | no |
| `h8` | H8 error page | `h8_error.html`, which the server answers with 403: its script and its image | no |
| `h16_click_nav` | H14, H16 navigation from a loaded page | `h16_a.html` (red) to `h16_b.html` (green): the user's click on a link, and four routes the page starts itself (`link.click()`, `location.href = url`, a submit button's `click()`, `form.submit()`) | the link click only |
| `h19_spin` | H19 a script that does not end (ebay hang, 2026-10-08) | `h19_spin.html`: a script inside nested `forEach` callbacks that would run for hours; the load must finish at the app's 60 s script budget (the check takes a minute) | no |
| `i0a_script` | I0(a) the window stays usable: a script that does not end | `i0a_spin.html`: ticks after the load; when the driver says so, a timer callback starts a loop that would run for hours | controls only |
| `i0a_layout` | I0(a): very expensive layout | `i0a_layout.html?n=150`: 3000 wrapping flex items, every one resized each 700 ms | controls only |
| `i0a_nav` | I0(a): repeated navigation while loading | `i0a_nav.html?n=1` to `?n=5`: each visit starts the next, then meets a script held 1.5 s and an image held 3 s | controls only |
| `i0a_close` | I0(a): closing a busy tab | `i0a_spin.html`, then Cmd+W | yes |
| `e4_contextmenu` | CH2.E4 right press | `e4_mouse.html?words`: `contextmenu` (button 2) reaches the page and the native menu opens (`hwdrive menus`); Escape closes it | yes |
| `e4_contextmenu_prevented` | CH2.E4 | `e4_mouse.html?words&prevent`: the page cancels it, no menu, the release is an `auxclick` | yes |
| `e4_dblclick_selects_word` | CH2.E4 click count | `e4_mouse.html?words`: `hwdrive dblclick`; `dblclick` with detail 2 finds "bravo" selected | yes |
| `e4_drag` | CH2.E4 | `hwdrive drag`: `pointermove` with `buttons` 1 | yes |
| `e4_cursor_hand` | CH2.E4 CSS cursor | `e4_mouse.html`: `hwdrive cursor` reads the system cursor: hand over the link, left-right over `ew-resize`, arrow in the title bar | yes |
| `e4_tooltip` | CH2.E4 `title` | the link's `title` is the view's tooltip (debug log) and a tooltip window shows after a rest | yes |
| `e9_redirect` | CH2.E9 redirect | `e9_start` redirects to `e9_final.html`: the saved tab holds the committed URL and the title | no |
| `e9_redirect_webkit` | CH2.E9, `HIWAVE_RUSTKIT_CHROME=0` | as `e9_redirect`, on the WebKit chrome (the escape hatch) | no |
| `a7_no_bridge_in_content` | CH3.A7 | `a7_bridge.html`: the page sees no `window.ipc` / `window.hiwaveChrome`, with `HIWAVE_RUSTKIT_CHROME=0` and by default | no |
| `e4_chrome_tab` | CH2.E4, RustKit chrome (the default) | hand over a chrome tab; a right press on it reaches chrome.html's handler (Tab Actions panel in the saved frame), no native menu | yes |
| `e19_icon_hover` | CH2.E19 | `e19_icon_hover.html`: an inline svg icon filled red, green by `#b:hover svg path`; the frame at rest, under the pointer, and off the button | yes |
| `e22_focus_ring` | CH2.E22 | `e22_focus_ring.html`: the chrome's toolbar row before and after a click in the address bar (its `:focus-within` accent border); a field bordered red, green by `#f:focus`, before and after Tab from the page's blank half | yes |
| `b11_settings_save` | CH2.B11 Save Settings | `e9_final.html`; the sidebar's Settings, the 'Sidebar Open on Startup' toggle and Save Settings, found by `hwdrive find` (Accessibility) and clicked; asserts settings.json changed and that the page said 'Settings saved!' only when it was written | yes, and Accessibility |

### The I0(a) checks

Each of the four makes a page misbehave and asks whether the person still has
control. There are three fixture pages for the four checks: `i0a_script` and
`i0a_close` both use `i0a_spin.html`. `i0a_script` also asks that a request
the page made in the turn its script was stopped in is still answered to it. Two measures, reported apart:

- **The stall log** (needs nothing from the seat). The driver starts the app
  with `HIWAVE_STALL_LOG=1`; a helper thread in the app then posts a ping to
  the window's event loop every 100 ms and logs `Window thread holding ms=N`
  and `Window thread held ms=N` for any ping that waited 200 ms or more
  (`crates/hiwave-app/src/window_stall.rs`). While a ping waits, a click, a
  key, a scroll or a resize waits too. A hold can be up to 100 ms longer than
  the figure. With the flag on the loop wakes ten times a second, so the other
  checks leave it off.
- **The controls** (input, unlocked screen): Cmd+T, a resize and, in
  `i0a_close`, Cmd+W, each answered within 200 ms. **None of these has run
  once.** A quiet stall log is not a pass for them: a loop that turns can
  still draw nothing.

Timers a page sets during its load are run by the load itself, so
`i0a_spin.html` waits for the driver's word (a beacon answered with `spin`)
before it starts. Stop, tab switch and scrolling another tab are not driven
yet: the app has one content view, so there is no other tab to scroll.

Pages use targets that fill the viewport or flat colours counted over the
whole frame, so no check depends on where the content view sits inside the
window.

## Status (2026-10-05)

Run so far only on a seat with **no grants and a locked screen**: the
request-log and app-log assertions have run; `hwdrive`'s `preflight` and
`window` have run. **Nothing that posts an event, resizes the window or
captures a frame has been executed once**: `hwdrive move/click/press/release/
scroll/key/type/resize/activate/request`, the driver's `frame()` and every
assertion on pixels. Expect to fix things the first time they run.

Limits: one display scale seen (1x); the app is started as a bare binary, not
a bundle; one window; the fixture is plain HTTP on 127.0.0.1.
