# HiWave Inspector event log

The engine keeps a per-view, ordered record of what a page load did: what
Chrome's Console panel shows (console calls, uncaught errors, unhandled
rejections), what the engine does not implement that the page used, and the
navigation and lifecycle markers, and what Chrome's Network panel shows: every
request the engine makes, and exactly one response or failure for it.

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
| `script_error` | `name` (a standard name, else `Error`; empty for a thrown non-error), `message` (`null` unless kept, below), `location`, `stack`, `unhandled_rejection`, `script`, `value` (what was thrown: `error`, `string`, `number`, `bigint`, `boolean`, `symbol`, `null`, `undefined`, `object`, `array`, `function`; `engine` for the engine's own failure), `message_len` (characters in an error's message or a thrown string, else `null`) |
| `unsupported_api` | `interface`, `member`, `location` (first sighting on the page) |
| `first_layout` | — |
| `first_paint` | — |
| `relayout` | `cause` (engine words only, below), `took_ms` |
| `navigation_redirect` | `from`, `to`, `status`: one per redirect the document followed, between its `request` and `response` |
| `request` | `id` (per view, from 1), `url`, `method`, `resource_type`, `initiator` |
| `response` | `id`, `status`, `mime` (essence only, `type/subtype`; `null` when the header is not a valid one), `encoded_size`, `decoded_size`, `from_cache`, `duration_ms` |
| `failure` | `id`, `stage`, `reason`, `duration_ms` |

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

`message` of a `console` event is built when the call is made, by the native
`console.*` method itself, and runs none of the page's code. By default it holds no page text: a string is
`<string len=N>`, a number `<number>` (`<bigint>`, `<symbol>`), `true`,
`false`, `null` and `undefined` as they are, an error
`<TypeError message len=N>` (a name the engine does not give is `Error`), any
other object a tag (`[object Object]`, `[object Array]`,
`[object Function]`). With `InspectorConfig::keep_console_text`
(`parity-capture --event-log-console-text`) it is the text: primitives as
they are, `Symbol(x)`, an error as `Name: message` from its own data
properties, objects as the same tags, and URLs in it scrubbed (see Privacy).

`script` names what ran the code: a script URL, `inline#n`, or the engine
step (`event:DOMContentLoaded`, `event:load`, `event:resize`,
`event:<mouse event>`, `timers`, `network`). An error thrown by a listener
that a script's own dispatch ran (`button.click()`) is that script's, recorded
when the script ends (after its console lines). A module's error is recorded
once, never also as an unhandled rejection, including one thrown after a
top-level `await` that outlived the module's script step (recorded when the
step that settled it ends, as the module's).

A `script_error` holds no page text by default. It is described in native
code from the thrown value itself, never from its text, and runs none of the
page's code (no `toString`, getter, `toJSON`, `Symbol.toPrimitive` or Proxy
trap):

- `value` is the thrown value's type, and `message_len` the length of an
  error's `message` (an own or inherited data property, never a getter) or
  of a thrown string. A Symbol's description, a number and an object's
  properties are not recorded.
- `name` is the error's `name` only when it is one the engine itself gives:
  `Error`, `TypeError`, `RangeError`, `SyntaxError`, `ReferenceError`,
  `EvalError`, `URIError`, `AggregateError`, `DOMException`, and the
  `DOMException` names the bindings throw (`AbortError`, `DataCloneError`,
  `HierarchyRequestError`, `IndexSizeError`, `InvalidCharacterError`,
  `InvalidStateError`, `NamespaceError`, `NoModificationAllowedError`,
  `NotAllowedError`, `NotFoundError`, `NotSupportedError`, `OperationError`,
  `QuotaExceededError`, `SecurityError`, `TimeoutError`,
  `TypeMismatchError`, `NetworkError`, `EncodingError`,
  `InvalidAccessError`; `rustkit_js::STANDARD_ERROR_NAMES`). Any other name
  is `Error`.
- `message` is `null` for every error that reached page script, whoever
  raised it: page code can catch an engine error and rethrow it, or throw
  the engine's very words (`Failed to fetch`, `x is not defined`), so no
  message is ever kept, or chosen, because of what its text says. Only the
  engine's own failures, which the engine marks where it raises them
  (`EngineFailure`, never by their text), have a message: `JS engine panic`
  or `import map rejected`, with `value` `engine`.
- `stack` is the page's frames, innermost first, one `    at url:line:col`
  line each, capped at 4096 characters: no function names (a page can name a
  function anything), and only frames in a script the engine evaluated
  (`inline#n`, or the script's URL as in Privacy). A frame that cannot be
  read (no position, an unknown script, a native or `eval` frame) is
  dropped, never copied. An error object's frames are the engine's record of
  where it was made, read through the engine's own `Error.prototype.stack`
  getter, never a `stack` the page set.

With `InspectorConfig::keep_error_text` (`parity-capture
--event-log-error-text`) `message` is the page's text, verbatim, capped at
1024 characters: an error's `message`, a thrown string, `Symbol(x)`, a
number. It carries whatever the page put in it (cookies, form values,
tokens): never for a log that will be shared.

`cause` of a `relayout` names why the view was laid out again, in the
engine's words: `initial_load`, `stylesheet_loaded`, `image_loaded(count=N)`,
`font_loaded`, `scroll`, `resize`, `timer`, `observer_callback`,
`hover_or_focus`, `input`, `other(<engine name>)`, or a script write,
`script_mutation(<kind>)` (a text node) or `script_mutation(<kind>@<tag>)`,
where `<kind>` is `attribute`, `child_list`, `text`, `style` or `form_value`
and `<tag>` is the element's tag when it is a standard HTML element (or `svg`,
`math`), else `other` (a custom element's name, an unknown tag). An element's
`id`, class, attribute names and values are never recorded. (The engine's
`relayout cause=` trace line, which is not the Inspector's, keeps `tag#id`.)

### Requests

Every request the engine makes is a `request`, and exactly one `response` or
`failure` with the same `id` follows (an engine test checks this on every
network test; a debug assertion catches a second answer). The engine records a
request through a guard that answering consumes and that records a failure if
it is dropped unanswered, so a fetch whose future is dropped (a budget
deadline, a navigation) is still answered.

`resource_type`: `document`, `stylesheet`, `script` (classic), `module`
(a module script or an import), `image`, `font`, `fetch`, `xhr`, `other`.

`initiator`: `navigation` (the document), `parser` (the document's markup),
`css` (a stylesheet's `url()`: fonts, background images), `script` (page
script: `fetch`, XHR, `import`, images scripts added).

A response with an error status (`404`) is a `response`: the server answered.
`encoded_size` is the body as transferred (before gzip/deflate were undone),
`null` from the cache or for script requests; `decoded_size` is what the
engine read, `null` when it did not read the body.

`failure.stage`:

| stage | when |
|---|---|
| `dns` | the host name did not resolve |
| `connect` | no connection, or the connection broke mid-body |
| `tls` | the TLS handshake failed |
| `timeout` | the network did not answer in time (live script requests: 30 s) |
| `http_error` | not usable HTTP (malformed, too many redirects) |
| `policy` | refused by a rule: the shield, the private-network rule, mixed content, CSP, a malformed or oversized request, a request cap |
| `cors` | a script request another origin did not allow (or a `same-origin` request to another origin) |
| `mime` | a module script not served with a JavaScript MIME type |
| `decode` | the body arrived and was not usable: not UTF-8, an image that does not decode, an SVG that does not parse |
| `cancelled` | abandoned before its budget ran out: the page navigated away or the view closed (reason `abandoned (navigation or view closed)`), or the navigation was superseded |
| `over_budget` | the subresource phase's or the script budget ran out first, or the response was over the size cap |

Not recorded as separate requests: CORS preflights and the hops of a script
request's redirects (the policy follows them inside one request), `data:` URLs
of images, stylesheets and fonts, and images the image cache already holds. A
`fetch()` or XHR of a `data:` URL is a request, shown as `data:<mime>,…`.

A previous page's live script requests are dropped when the new document is
committed, so their `cancelled` failures come after the new page's
`navigation_start` (and its document request), before its `navigation_commit`.
If the new document cannot be fetched, the old page and its requests stay.

### Unsupported APIs

Recorded where the bindings know they do not implement something and page
script reaches it: `XMLHttpRequest.open(async=false)`,
`XMLHttpRequest.responseXML` (read on a finished response whose type is XML,
or HTML with `responseType = 'document'`: anywhere else it is `null` in
every browser), `CustomElementRegistry.define(extends)`,
`ReadableStream.getReader(byob)`, `DOMParser.parseFromString(XML)`. Only
these members are ever recorded (`rustkit_bindings::UNSUPPORTED_MEMBERS`),
through a native reporter page script cannot reach; the table holds at most
64 members. One event
per member per page; `Inspector::unsupported_apis()` and the summary carry the
counts. Property reads that fall through to `undefined` are not detected:
that needs a hook on every property access (see the PR that added this).

## Privacy

The log is meant to be pasted into public issues and PRs.

- Every URL keeps its scheme, host, port and path. User name and password are
  always removed; query and fragment are removed unless
  `InspectorConfig::keep_url_query` (`parity-capture --event-log-keep-query`)
  is set. A `data:` URL becomes `data:<mime>,…`; a `blob:` URL its origin.
- A frame's or a script's URL is read by the URL parser (any scheme; tabs
  and newlines dropped, backslashes as slashes, IPv6 hosts) and reduced as
  above; a `data:` script is `data:<mime>,…`, a scheme with no host its
  scheme alone (`javascript:`). What the parser cannot read is dropped.
- Errors carry no page text by default (see `script_error` above): the
  thrown value's type, a standard name, a message's length, frame positions.
  Messages are not scrubbed: they are left out. Only the engine's own
  failures, marked where the engine raises them, carry a fixed message.
- A `relayout` cause is engine words, a mutation kind and a standard HTML tag
  name; never an element's `id`, class, attribute or custom element name.
- No other field holds a name the page chose: event types in `script` are
  the ones the engine dispatches itself (`load`, `DOMContentLoaded`,
  `resize`, mouse and pointer events); `unsupported_api` members are a fixed
  list; console methods other than `log`, `info`, `warn`, `error` and
  `debug` (`group` labels, `time` and `count` labels) and
  `performance.mark` names are not recorded. `InspectorConfig::keep_error_text` keeps the page's
  text: never for a log that will be shared.
- With `keep_console_text` on, URLs in the kept console text are scrubbed
  wherever they sit in it (after a quote, in brackets, after `=`): a
  convenience, not a guarantee; the text is the page's.
- No request or response header, body, cookie, form value or storage content
  is recorded: a request has its method and URL, a response its status, MIME
  essence and sizes. Failure reasons are fixed engine phrases, never the
  network error's own text (which can name hosts and paths).
- Nothing page script can reach switches redaction off, writes into the
  console, error or unsupported-member queues, or changes what a record
  says. The `console` methods are native functions; the redaction flag and
  the queues live in Rust (`rustkit_js` `diag`). The bindings' own scripts
  report errors and unsupported members through natives handed to them in a
  closure before any page script runs (`JsRuntime::evaluate_with_host`):
  there is no window global, no property on `console`, and no call through
  a prototype or global a page can patch (`Array.prototype.push`,
  `Function.prototype.apply`, `JSON.stringify`, `String`, `toString`,
  `toJSON`, getters, `Symbol.toPrimitive`) between a page value and a
  record.
- Every queue is bounded: console lines between flushes (1024, oldest
  dropped), error reports (256), listener error texts for the script log
  (1024), unsupported members (64), unhandled rejections (256), the engine's
  own promises (1024), known script paths (4096), and the ring buffer.
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
  "schema": 2,
  "events": 42,
  "console": { "log": 3, "info": 0, "warn": 1, "error": 2, "debug": 0 },
  "script_errors": 4,
  "unhandled_rejections": 1,
  "unsupported_apis": { "XMLHttpRequest.open(async=false)": 3 },
  "requests": { "document": { "2xx": 1 }, "image": { "2xx": 12, "4xx": 1, "failed": 2 } },
  "failures_by_stage": { "over_budget": 1, "policy": 1 }
}
```

| field | meaning |
|---|---|
| `schema` | layout version (2; 1 had no `requests` or `failures_by_stage`) |
| `events` | records in the load's log |
| `console` | console calls by level |
| `script_errors` | uncaught exceptions plus unhandled rejections |
| `unhandled_rejections` | of `script_errors`, the rejections |
| `unsupported_apis` | `Interface.member` to how often page script reached it |
| `requests` | resource type to outcome to count; outcome is the response's status class (`2xx`, `3xx`, `4xx`, `5xx`, `other`) or `failed` |
| `failures_by_stage` | failed requests by `failure.stage` |
