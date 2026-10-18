# Replay harness

Serve a HAR to HiWave and to Chrome, then diff the two page traces. Recorded
HARs contain cookies. Keep them outside the repo. `fixtures/synthetic.har` is
the only archive that belongs here.

## Replay server

```bash
python3 tools/replay/server.py captured.har --port 8765 --time-scale 1
```

Requests match on method plus a normalised URL: lowercase scheme and host,
default port removed, fragment removed, query pairs sorted. The nth request
for a key replays the nth entry, then repeats the last. A miss is logged as
`MISS METHOD url` (no cookie or authorization values) and answered 404.

`www.redditstatic.com/js/concat?prefix=…&chunks=id,id,…` is built by the
page, so the chunk list often matches no recorded URL. The server parses
those ids and, when a recorded body is `SML.dm("id", …)` registrations
separated by `/* @sml-chunk */`, serves the requested chunks in that order
(`CONCAT split` on stderr). When the body does not split that cleanly it
sends the smallest set of recorded concat bodies that cover the request
(`CONCAT cover`). A chunk that is in neither is logged as `CONCAT missing <id>`.

Recorded response headers are sent. Hop-by-hop headers are dropped and
`Content-Length` is the body that is written. The entry's `time` (milliseconds)
is slept, multiplied by `--time-scale` (`0` sends immediately).

HiWave sends its own requests when `HIWAVE_REPLAY` is set. It is off when the
variable is unset.

```bash
HIWAVE_REPLAY=http://127.0.0.1:8765 cargo run -p hiwave-app --features rustkit
```

Chrome uses the same process as an HTTP proxy. HTTPS entries are answered on
`CONNECT` with a temporary certificate, so Chrome needs
`--ignore-certificate-errors` (the comparison script sets that).

## Traces

`tools/replay/chrome_trace.mjs` loads one URL through the proxy and writes JSON:

```bash
node tools/replay/chrome_trace.mjs \
  --replay http://127.0.0.1:8765 \
  --url https://example.test/ \
  --out /tmp/chrome.json
```

HiWave writes the same schema for the load when `HIWAVE_TRACE_JSON=/tmp/hiwave.json`
or `HIWAVE_CONSOLE_LOG=1` (one `PAGE_TRACE` line on stderr). Both together
write the file and the line. The file is replaced as the load proceeds, and
again when the load returns.

Fields: `browser`, `url`, `shield` (HiWave's `HIWAVE_SHIELD` choice;
`off (HIWAVE_REPLAY defaults HIWAVE_SHIELD=off)` when replay is on and the
shield was left unset; Chrome writes `off (stock Chrome)`), `ready_state`
(`loading` / `interactive` / `complete` with `t_ms` from navigation start),
`dcl_ms`, `load_ms`, `scripts` (`execute`, then `load` or `error`),
`fetched_scripts` (HiWave: each parser script and module dependency, with
`executed` and, if it did not run, `reason` of `blocked`, `module graph error`,
`waiting on a dependency`, or `never reached`; `waiting_on` names the
outstanding import when the reason is `waiting on a dependency`), `console`
errors, `mutations`
(one record per subtree add or remove of more than 50 nodes), `max_js_stack`.
The diff does not compare `shield`, `fetched_scripts`, or `parser` durations.
`fetched_scripts` also carries `detail` when a script was missed for a reason
the four `reason` values do not name: `shadow tree`, `not a javascript type`,
`template`, `noscript`, `inserted after the parser snapshot`, or
`script-inserted inline`. An external script carries `inline` (the `inline#N`
slot it consumed) so a gap in the inline labels names that script.
`module_timeline`, when any module was prefetched, is one record per URL:
`requested_ms`, `start_ms`, `body_ms`, `parsed_ms`, and `held_ms` (how long
the body sat ready before the main thread noticed it, which can be during
another module's scan). `body_ms` is that observation, not the moment the
prefetch thread finished the read. `waiting_on` on a
`waiting on a dependency` row is that module's outstanding imports.
Inline scripts the parser inserted are labeled `inline#N` in document order,
counting external scripts, including ones inside a shadow tree, a `<template>`
content fragment, or `<noscript>`, and ones whose type is not javascript
(`type:<essence>#N`, which does not take an `inline#N`). A `template` script
is inert until its content is inserted or cloned. A `noscript` script does
not run while scripting is on.

`parser` is one record per pause: `blocked_on` is `stylesheet`, `font`,
`script fetch`, `script execute`, or `image`, with `ms` and `target`.
`font` and `image` are recorded only when that stall was at least 1ms.
The `image` pause is the wait still left before `window` `load`, not a wait
before the first inline script. Chrome fills stylesheet and
script fetch from resource timing. HiWave also records each script execution.
The trace stays open after a yielded load returns at DOMContentLoaded, until
the page has been quiet for two seconds or `HIWAVE_DUMP_RENDER_AT_MS` has
passed, whichever is later.

`HIWAVE_DUMP_RENDER=<path>` writes `{ "dumps": [...] }` for `main#main-content`.
The default writes each of the first 3 display-list commits after a large
insertion into `MAIN`, and one more when the load is quiet.
`HIWAVE_DUMP_RENDER_AT_MS` adds one dump at that many milliseconds after
navigation start. Each dump is the path from `<main id=main-content>` to the
first three `shreddit-post` elements, plus their direct children: tag, id,
class, custom-element state (`undefined`, `defined`, `failed`, `custom`, and
`uncustomized` for a built-in), the hidden attribute, shadow root
(`mode`, `child_count`, `adopted_stylesheets`), slot assignment, computed
display, visibility, opacity, content-visibility, position, height, and
overflow, the layout box or `"no box"`, and whether the element was in the
last display list. `display_list_items_in_main` is how many display-list
commands were emitted inside main. `skip` is why a subtree was not painted:
`no box`, `zero size`, `clipped`, `display:none`, `content-visibility`, or
`unslotted light DOM`. The same object is copied onto the trace as `render`.
Chrome writes `render` from `DOM.getDocument`, `CSS.getComputedStyleForNode`,
and `DOM.getBoxModel`. Its `in_display_list` means the box model exists and
the element is not `display:none`, `visibility:hidden`, or `opacity:0`, and
its command count is the number of dumped elements that paint. The diff
compares the tree and ignores the rect numbers and that count. `defined` and
Chrome's `custom` compare equal.

```bash
python3 tools/replay/diff_trace.py --chrome /tmp/chrome.json --hiwave /tmp/hiwave.json
```

The report labels the two sides `chrome` and `hiwave`.

The replay server sleeps each HAR entry's `time` once, multiplied by
`--time-scale`, then writes the body in one shot. A concat response uses
the max of its source entries' times, not the sum and not a sleep per
chunk.

The diff prints the first structural mismatch and ignores timestamps. A `load`
event only one side recorded is skipped when both sides executed that script.
`max_js_stack` is compared last, and only when the depths differ by more than
16 frames and one is more than double the other.

## Tests

```bash
python3 tools/replay/test_replay.py
```
