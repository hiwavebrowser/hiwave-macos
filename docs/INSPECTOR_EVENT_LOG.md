# HiWave Inspector event log

The engine keeps a per-view, ordered record of what a page load did: what
Chrome's Console panel shows (console calls, uncaught errors, unhandled
rejections), what the engine does not implement that the page used, and the
navigation and lifecycle markers. The Network half (requests and their
outcomes) is documented in the section added with it.

Code: `crates/rustkit-engine/src/inspector.rs` (types, buffer, sinks) and
`inspector_glue.rs` (where page script reaches it).

## Turning it on

Nothing is recorded unless something listens. With nobody listening every
recording call returns after one atomic load: no event is built, no string
formatted.

- `EngineConfig::inspector.buffer = true`: each view keeps its last
  `capacity` events (default 4096) in a ring buffer; past it the oldest is
  dropped and counted.
- `Engine::add_inspector_sink(view, sink)`: every event also goes to the sink
  (`JsonLinesSink`, `RollingFileSink`, or your own `InspectorSink`). Attach
  before loading: console capture and rejection tracking are set up when a
  document is committed.
- `Engine::inspector(view)` returns an `Arc<Inspector>` that can be kept and
  read from any thread, including while the page's script runs:
  `snapshot_since(n)` returns the buffered records with `seq >= n`, the next
  `seq`, and how many records the buffer has dropped. Reading runs no script.

## Records (JSON Lines)

One JSON object per line. Every record starts with:

| field | meaning |
|---|---|
| `seq` | 0-based, per view, consecutive, never reused |
| `t_ms` | milliseconds since the current navigation started (monotonic) |
| `type` | the event kind, below |

Then the kind's fields, in this order. Fields are only ever appended; a
field that is `null` today may hold a value later.

| `type` | fields |
|---|---|
| `navigation_start` | `url` (`null` for `load_html`) |
| `navigation_commit` | `url`, `status` (HTTP status; `null` for `load_html`) |
| `load_finished` | `url`, `scripts_over_budget` (scripts not run, or stopped, because the script budget was spent) |
| `load_ended_early` | `url`, `reason` (`navigated_away`, `failed`), `detail` (`stopped`, `another navigation started`, `HTTP 404`, or a one-word network stage: `dns`, `connect`, `tls`, `timeout`, `policy`, `redirect`, `http`, `network`, `invalid_url`, `over_budget`, `cancelled`) |
| `console` | `level` (`log`, `info`, `warn`, `error`, `debug`), `message`, `truncated`, `location` |
| `script_error` | `name` (`TypeError`, ...; empty for a thrown non-error), `message`, `location`, `stack`, `unhandled_rejection`, `script` |
| `unsupported_api` | `interface`, `member`, `location` (first sighting on the page) |
| `first_layout` | — |
| `first_paint` | — |
| `relayout` | `cause` (the value of the `relayout cause=` log line), `took_ms` |

`location` is `null` or `{ "url", "line", "column" }` (1-based). `url` is the
script's URL, or `inline#n` for the page's n-th `<script>` when it is inline,
classic or module (the line then counts from the start of that script's text;
Chrome counts from the start of the document). Lines and columns are the JS
engine's (Boa), with one correction: a console call is placed at the method's
name (`console.log(` at `log`), as Chrome places it. Otherwise a call is placed
at its `(`, a property access at the property name, and an error raised by a
name lookup inside a function can carry the function's start instead (a Boa
limitation).

Code that is in no named script has no `location`: `eval` code and the handler
an `on*` attribute compiles. Their console calls and errors are never given
the position of the code that called them; an error's `stack` still lists the
page frames around them.

`message` of a `console` event is built when the call is made and runs none
of the page's code. By default it holds no page text: a string is
`<string len=N>`, a number `<number>` (`<bigint>`, `<symbol>`), `true`,
`false`, `null` and `undefined` as they are, an error
`<TypeError message len=N>` (a name the engine does not give is `Error`), any
other object a tag (`[object Object]`, `[object Array]`,
`[object Function]`). With `InspectorConfig::keep_console_text`
(`parity-capture --event-log-console-text`) it is the text: primitives as
they are, `Symbol(x)`, an error as `Name: message` from its own data
properties, objects as the same tags, and URLs in it scrubbed (see Privacy).
Unhandled rejections are described as the kept text is.

`script` names what ran the code: a script URL, `inline#n`, or the engine
step (`event:DOMContentLoaded`, `event:load`, `event:resize`,
`event:<mouse event>`, `timers`, `network`). An error thrown by a listener
that a script's own dispatch ran (`button.click()`) is that script's, recorded
when the script ends (after its console lines). A module's error is recorded
once, never also as an unhandled rejection.

A thrown value that is not an error is described as kept console text
describes it (`[object Object]`, a string as it is), never with its
properties; `name` is then empty.

`stack` holds the page's frames only (`    at name (url:line:col)` lines; no
native, engine or `eval` frames), capped at 4096 characters.

### Unsupported APIs

Recorded where the bindings know they do not implement something and page
script reaches it: `XMLHttpRequest.open(async=false)`,
`XMLHttpRequest.responseXML` (read on a finished response whose type is XML,
or HTML with `responseType = 'document'`: anywhere else it is `null` in
every browser), `CustomElementRegistry.define(extends)`,
`ReadableStream.getReader(byob)`, `DOMParser.parseFromString(XML)`. One event
per member per page; `Inspector::unsupported_apis()` and the summary carry the
counts. Property reads that fall through to `undefined` are not detected:
that needs a hook on every property access (see the PR that added this).

## Privacy

The log is meant to be pasted into public issues and PRs.

- Every URL keeps its scheme, host, port and path. User name and password are
  always removed; query and fragment are removed unless
  `InspectorConfig::keep_url_query` (`parity-capture --event-log-keep-query`)
  is set. A `data:` URL becomes `data:<mime>,…`; a `blob:` URL its origin.
- URLs inside error messages, rejection reasons, stacks and kept console
  text are scrubbed the same way wherever they sit in the text (after a
  quote, in brackets, after `=`). Once a URL's query or fragment starts,
  everything up to whitespace goes with it (brackets, parens and quotes are
  legal in a query), except a frame's `:line:col)` and closing punctuation.
  A `data:` URL in text becomes `data:<mime>,…` and the rest of its line
  goes with its payload.
- No request or response header, body, cookie, form value or storage content
  is recorded.
- Console messages carry no page text by default: each argument's type, and
  a string's length. Pages log cookies, form values and their own URL.
  `InspectorConfig::keep_console_text` (`parity-capture
  --event-log-console-text`) keeps the text, capped at 1024 characters,
  URLs in it scrubbed, never interpreted: never for a log that will be
  shared.

## `parity-capture --event-log <file> --event-summary`

`--event-log` writes the whole load's records (nothing dropped) as JSON Lines.
`--event-summary` adds `event_summary` to the result JSON printed on stdout.
The site board publishes it per site, so its layout is stable: fields keep
their names and order, maps are sorted by key, new fields are only appended,
and `schema` goes up when one is.

```json
{
  "schema": 1,
  "events": 42,
  "console": { "log": 3, "info": 0, "warn": 1, "error": 2, "debug": 0 },
  "script_errors": 4,
  "unhandled_rejections": 1,
  "unsupported_apis": { "XMLHttpRequest.open(async=false)": 3 }
}
```

| field | meaning |
|---|---|
| `schema` | layout version (1) |
| `events` | records in the load's log |
| `console` | console calls by level |
| `script_errors` | uncaught exceptions plus unhandled rejections |
| `unhandled_rejections` | of `script_errors`, the rejections |
| `unsupported_apis` | `Interface.member` to how often page script reached it |
