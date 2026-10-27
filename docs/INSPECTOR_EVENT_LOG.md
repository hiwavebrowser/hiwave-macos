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
| `console` | `level` (`log`, `info`, `warn`, `error`, `debug`), `message`, `truncated`, `location`, `stack` (`console.error` and a failed `console.assert` only: the caller's page frames, written as `script_error`'s `stack`; else `null`), `error_stack` (`console.error` with an error among its arguments, as a framework that catches and logs errors calls it: the first one's own frames, where it was made; else `null`) |
| `script_error` | `name` (a standard name, else `Error`; empty for a thrown non-error), `message` (`null` unless kept, below), `location`, `stack`, `unhandled_rejection`, `script`, `value` (what was thrown: `error`, `string`, `number`, `bigint`, `boolean`, `symbol`, `null`, `undefined`, `object`, `array`, `function`; `engine` for the engine's own failure), `message_len` (characters in an error's message or a thrown string, else `null`), `inserted_by` (when the innermost frame's script is a `<script src>` that page script inserted: the `location` of the insertion call, else `null`; `null` too when `location` is, as for an `eval` or `on*` throw) |
| `unsupported_api` | `interface`, `member`, `location` (first sighting on the page) |
| `first_layout` | — |
| `first_paint` | — |
| `relayout` | `cause` (engine words only, below), `took_ms` |
| `navigation_redirect` | `from`, `to`, `status`: one per redirect the document followed, between its `request` and `response` |
| `request` | `id` (per view, from 1), `url`, `method`, `resource_type`, `initiator` |
| `response` | `id`, `status`, `mime` (essence only, `type/subtype`; `null` when the header is not a valid one), `encoded_size`, `decoded_size`, `from_cache`, `duration_ms`, `queued_ms` (see below) |
| `failure` | `id`, `stage`, `reason`, `duration_ms`, `blocked_by` (when the shield refused it and said which: `{ "list": "EasyList", "rule": "<the rule as the list writes it, ≤256 chars, or null>" }`, the filter list's own public text; else `null`. Rules the filter engine joined into one are written each as its list writes it, separated by ` <+> `, and any of them may have matched: `list` is then named only when all are from that list, else `unattributed`. The rule is named only where the tool asked for rule names (parity-capture `--shield`; they cost about 8 MB); elsewhere `rule` is `null` and `list` is `unattributed` when several lists are loaded), `queued_ms` |
| `script` | `script`, `kind`, `outcome`, `reason`, `bytes`, `queued_ms`, `fetch_ms`, `waited_ms`, `ran_ms`, `event`, `held`: one page script's life, recorded when it is over (see Scripts) |
| `lifecycle` | `event` (`DOMContentLoaded`, `load`), `held_by`, `held_by_count`: recorded as the event fires (see Scripts) |

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

While somebody listens, a `console` event is recorded when the call is made,
so a reader on another thread (the devtools panel) sees the console of a
script that is still running. That is decided per call, at most 200
hand-overs a second. A hand-over also records the calls held before it, so
this caps hand-overs, not records: up to about 1,224 calls a second can be
recorded. Past the cap, and with nobody listening, calls wait for the
script's end (the `HIWAVE_CONSOLE_LOG` mirror alone included). Either way each
call is recorded once, in the order it was made. A script that hangs after a
hot loop never reaches its end, so how many calls are held is readable
without the page (`Inspector::console_held`), and a panel can say "K console
lines held by a running script".

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
  line each, capped at 4096 characters: no function names unless error text
  is kept (a page can name a function anything; with it a named function's
  frame is `    at name (url:line:col)`, the name capped at 128 characters,
  and top-level or unnamed code stays `    at url:line:col`; a computed
  name holding a control character or a parenthesis is dropped, so it
  cannot forge a frame line). A call frame
  is placed at its `(` (Boa's place; Chrome's is the callee's name). Only
  frames in a script the engine evaluated
  (`inline#n`, or the script's URL as in Privacy). A frame that cannot be
  read (no position, an unknown script, a native or `eval` frame) is
  dropped, never copied. An error object's frames are the engine's record of
  where it was made, read through the engine's own `Error.prototype.stack`
  getter, never a `stack` the page set.

With `InspectorConfig::keep_error_text` (`parity-capture
--event-log-error-text`) `message` is the page's text, verbatim, capped at
1024 characters: an error's `message`, a thrown string, `Symbol(x)`, a
number. It carries whatever the page put in it (cookies, form values,
tokens): never for a log that will be shared. Messages and function names
follow this Inspector setting only: `HIWAVE_CONSOLE_LOG=1` keeps the
runtime's text for the tracing log, and none of that text reaches the
event log unless `keep_error_text` is on.

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
`css` (a stylesheet's `url()`: fonts, background images), `fetch` (page
script's `fetch()`), `xhr` (an `XMLHttpRequest`), `inserted_script` (a
`<script src>` page script inserted; `script_error.inserted_by` says where
from), `script` (other page script: `import`, images scripts added). Before
dev-tools T2, `fetch`, `xhr` and `inserted_script` were all `script`.

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
| `timeout` | the network did not answer in time (live script requests: 30 s). Reason `in-flight cap` when the deadline passed while the request was still queued for a slot (see `queued_ms`) |
| `http_error` | not usable HTTP (malformed, too many redirects) |
| `policy` | refused by a rule: the shield, the private-network rule, mixed content, CSP, a malformed or oversized request, a request cap |
| `cors` | a script request another origin did not allow (or a `same-origin` request to another origin) |
| `mime` | a module script not served with a JavaScript MIME type |
| `decode` | the body arrived and was not usable: not UTF-8, an image that does not decode, an SVG that does not parse |
| `cancelled` | abandoned before its budget ran out: the page navigated away or the view closed (reason `abandoned (navigation or view closed)`), or the navigation was superseded |
| `over_budget` | the subresource phase's or the script budget ran out first, or the response was over the size cap |

`queued_ms`: of `duration_ms`, how long the request waited for an in-flight
slot. The caps queue a request and never refuse it: a script request (`fetch`,
XHR, a module) waits for one of 6 per origin and 24 per page, and an inserted
`<script src>` for one of the process's 8. `null` for a request no cap queues
(the document, images, stylesheets, fonts, parser scripts) or one refused
before it queued. A request still queued when its deadline passed is a
`timeout` (or `over_budget`) with the reason `in-flight cap`, and its
`queued_ms` is the whole wait.

Not recorded as separate requests: CORS preflights and the hops of a script
request's redirects (the policy follows them inside one request), `data:` URLs
of images, stylesheets and fonts, and images the image cache already holds. A
`fetch()` or XHR of a `data:` URL is a request, shown as `data:<mime>,…`.

A previous page's live script requests are dropped when the new document is
committed, so their `cancelled` failures come after the new page's
`navigation_start` (and its document request), before its `navigation_commit`.
If the new document cannot be fetched, the old page and its requests stay.

### Scripts

Every page `<script>` the load path handles, and every `<script src>` page
script inserts, ends in exactly one `script` record: after it ran (and after
its console lines and errors), when the engine gave up on it, or as
`cancelled` when its document was left first (a navigation, a cancelled
load, a closed view). A navigation away while the page's own scripts are
still being fetched leaves those with no `script` record; their requests'
`failure` records (`cancelled`) carry it. Lifecycle
steps, timers and callbacks are not scripts here; their errors stay
`script_error`s.

| field | meaning |
|---|---|
| `script` | its URL (as Privacy reduces it), `inline#n`, or for a `src` that is not a URL `unparseable#n` (n: its place among the document's scripts); the text itself is never recorded. A script the page inserts with such a `src` is never queued: it fires `error` and gets no record |
| `kind` | `parser` (a parser-blocking classic script), `defer`, `async`, `module`, `inserted` |
| `outcome` | `ran`; `threw` (its `script_error` is above it); `skipped`; `fetch_failed`; `over_budget` (not started: the script budget was already spent); `stopped` (started, and stopped by the script budget's deadline); `engine_failed`; `cancelled` (its document was left before it ended) |
| `reason` | for every outcome but `ran` and `threw`, in the engine's words: `nomodule`, `too large to run inside the script budget`, `unparseable src`, `JS engine panicked earlier on this page`, `HTTP <status>`, `network` (the request's `failure` has the stage), `script budget`, `script budget spent before it started`, `JS engine panic`, `navigation or view closed` |
| `bytes` | source bytes; 0 when nothing arrived |
| `queued_ms` | `inserted` only: from the page inserting it to the engine taking it from the insertion queue, which starts its request. A large value means the page inserted it long before the engine next drained the queue |
| `fetch_ms` | from its request starting to its body or failure arriving; `null` for an inline script |
| `waited_ms` | from its body arriving to the engine starting it (or giving up on it): time spent behind other scripts, or waiting for a turn |
| `ran_ms` | parse, compile and run, its microtasks included (Boa does not separate them) |
| `event` | the event its element heard, or will hear: `load` or `error` (modules and inserted scripts; a parser classic script fires none). When the script budget is spent the event is owed to the next turn that has budget, ahead of any other script, and the record already names it |
| `held` | the first lifecycle event that had not fired when it ended: `DOMContentLoaded` (it ended before that fired), `load` (it ended between the two), or `null` (after `load`, or `cancelled`). An order, not a cause: see below |

The engine runs `async` page scripts after the deferred ones and before
`DOMContentLoaded` (Chrome runs them as they arrive and does not hold
`DOMContentLoaded` for them), so they read `held: DOMContentLoaded`.
Inserted scripts never hold `DOMContentLoaded`, yet one that ends before it
fired reads `held: DOMContentLoaded` too: `held` says when a script ended,
not that the event waited for it. A `lifecycle` record marks each event as
it fires; `load`'s `held_by` names the scripts that ended between the two
(first 16, in the order they ended) and `held_by_count` how many there
were. Same caveat: a script in `held_by` ended in that window, which is a
lead, not proof it held `load`. On a yielding load (the app) `load` fires
in a live turn once the inserted chain is idle, so there the scripts in
`held_by` are the chain that held it. The engine knows which events have
fired with nobody listening, so a panel attached after `DOMContentLoaded`
still reads the later scripts right; `held_by` lists only the scripts that
ended after it attached.

Cost: with nobody listening, each script costs one atomic load plus the
clock reads its times need (two per external page script, one when page
script inserts a script, one when its body arrives). Listening, one record
per script. The lifecycle flags cost one lock per navigation and per
lifecycle event either way.

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
  dropped; a `warn` line `K console calls dropped (more than 1024 held)`,
  with `; console rate cap` when the live cap held them, comes ahead of the
  ones kept), error reports (256), listener error texts for the script log
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
