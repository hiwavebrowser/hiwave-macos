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
`workspace_state.json`; the driver only swaps the tab URL in it.

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
