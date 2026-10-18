# Script loading rules

HiWave parses the document, then runs its scripts. That is not a streaming
tokenizer, but the run follows the same lists the HTML script processing
model uses. This note is the rule those lists implement.

Spec:

- [Prepare the script element](https://html.spec.whatwg.org/multipage/scripting.html#prepare-the-script-element)
- [Execute the script element](https://html.spec.whatwg.org/multipage/scripting.html#execute-the-script-element)
- [Parsing: script end tag](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-incdata) (the parser spin, and the pending parsing-blocking script)
- [The end](https://html.spec.whatwg.org/multipage/parsing.html#the-end) (stop parsing, the deferred list, `DOMContentLoaded`)
- [Fetch the descendants of a module script](https://html.spec.whatwg.org/multipage/webappapis.html#fetch-the-descendants-of-a-module-script)
- [Fetch a single module script](https://html.spec.whatwg.org/multipage/webappapis.html#fetch-a-single-module-script)
- [Delaying the load event](https://html.spec.whatwg.org/multipage/parsing.html#delaying-the-load-event)

## The four lists

**Pending parsing-blocking script.** At most one classic external script
without `async` or `defer`, plus an inline classic script that is still
waiting on a stylesheet. HiWave loads external stylesheets, then web
fonts, before the script walk (the `stylesheet` parser pause, and a
`font` pause when the font fetch took at least 1ms). It then awaits each
parser-blocking classic script when the walk reaches it, and records a
`script fetch` pause only for that wait. An inline classic script has no
fetch wait. It does not wait for an earlier `async` script, and it does
not wait for images.

**List of scripts that will execute in order as soon as possible.**
Script-created external classic scripts with the force-async flag clear
(`.async = false`, which also removes the `async` attribute). They run in
insertion order. A failure does not hold the next one. The parser does not
put scripts on this list: the parser clears force-async itself, and parser
scripts never go through the script-insert path.

**Set of scripts that will execute as soon as possible.** Parser-inserted
`async` scripts, and script-created external classic scripts that are still
force-async (the default). They do not block the parser, other parser
scripts, or `DOMContentLoaded`. HiWave runs the parser's `async` set after
`DOMContentLoaded` has been recorded and before `window` `load`, even when
the body arrived earlier. That is allowed (the "as soon as possible" task
can be queued after the parser finishes) and keeps the order stable. A slow
`async` fetch can delay `load`. It does not delay `DOMContentLoaded`, and
it is not a `script fetch` parser pause. A `script execute` pause is only
a parser-blocking classic script. A deferred module's fetch and run are
not parser pauses, even though `DOMContentLoaded` waits for that module.

**List of scripts that will execute when the document has finished
parsing.** External `defer` scripts and module scripts that are not
`async`, inline or external, in tree order. They run after the
parser-blocking list and before `DOMContentLoaded`. Their fetches start
during the stylesheet load and are awaited here, not before the first
inline script.

`nomodule` classic scripts are skipped. A script whose type is not
JavaScript (`application/ld+json`, and the rest) is not a script.

## DOMContentLoaded and load

`DOMContentLoaded` waits for the parser to finish, then the deferred list
(defer scripts and non-async modules). It does not wait for:

- `async` scripts
- script-inserted scripts
- `fetch()`, XHR, or images

Those network waits are answered after `readyState` becomes `interactive`.
`fetch()` and XHR callbacks still run before `load_url` returns, from the
pump that follows `DOMContentLoaded`.

Images are discovered with the document and their fetches start then, so
they progress during the script walk. The load awaits whatever is still
out only after `DOMContentLoaded` and before `window` `load`. A remaining
stall of at least 1ms is an `image` parser pause. The pause is the stall
at that point, not the whole image fetch: the request started with the
scripts. An inline classic script never waits on that fetch.

`window` `load` and `readyState` `complete` wait for the parser's `async`
set, for images, and for external scripts inserted while the document was
loading. Each inserted script runs and fires `load` or `error` first. A
script inserted from the load handler itself does not hold the event.

A yielding load (`load_url_yielding`) still returns once `DOMContentLoaded`
has fired and an inserted script is outstanding. Live turns run one arrived
body at a time and fire `load` when that chain is idle. Parser `async`
scripts run before that return, after `interactive` is recorded.

## Module graph

"Fetch the descendants of a module script": every static import of a module
is fetched concurrently. The graph is instantiated once those descendants
are present. HiWave does this in one walk:

- the fetch starts as soon as the specifier is known. A static `import`,
  `export from`, or string-literal `import("...")` is scanned before
  `Module::parse`, and the HTTP request starts then, on
  `rk-module-prefetch`. A computed `import(url)` starts when that call is
  evaluated, before the import job runs and before any later statement in
  the same script
- siblings discovered together are started together
- when one body arrives, its own static imports start before that body is
  parsed, while its siblings are still in flight

`module_timeline` records each of those URLs: `requested_ms` when the
specifier was known, `start_ms` when the prefetch was queued, `body_ms`
when the main thread took the bytes, `held_ms` when that body had already
been sitting ready on the prefetch thread, and `parsed_ms` when
`supply_module` returned. The gap between `requested_ms` and `body_ms` is
the fetch plus any wait until the main thread looked. `held_ms` is that
second wait: the loader waiting on the body, not JavaScript. A gap before
`requested_ms` is time until the specifier existed.

`parsed_ms` is not `Module::parse`. It is stamped when the loader comes
back, so one module's `parsed_ms` is the next module's `body_ms`. The work
inside that return is split out: `scan_at_ms` / `scan_ms` is the import
scan, `parse_at_ms` / `parse_ms` is `Module::parse` only, and `jobs_ms` is
`run_jobs` (the page's promise jobs and microtasks, including a graph that
resumed because this body arrived). `link_ms` and `eval_ms` are
`Module::link` and `Module::evaluate` for that graph root. The same slices
are `tasks` (`source`, `ms`, `url`, `t_ms`) and, while a window hold is
open, a `Window thread task` line per slice. Boa 0.22's `boa_gc` exposes
`force_collect` and no collection time, so a task has no `gc` source. A
phase of 50 ms or more is also a `hiwave::module` warning, and each phase
logs a start line so a silent stretch names the phase that is still
running.

A string-literal `import()` is queued in the same scan, but it does not
take a connection until the static imports from that scan have stored
their bodies. It is not a load dependency of the module, so it does not
delay `DOMContentLoaded`. A computed `import(url)` is not critical either.

The pump that runs a parser module, before `DOMContentLoaded`, fetches
only that module's static imports. A dynamic `import()` an earlier classic
script or an earlier module already evaluated is left queued. It is fetched
by the pump after `DOMContentLoaded`, with `fetch()` and XHR. Parsing it
inside the next deferred module's pump is what made a chunk requested
seconds into the load hold `DOMContentLoaded`: Chrome's probe
(`tools/replay/probe.js`, read by `chrome_trace.mjs`) records
`DOMContentLoaded` from `performance.now()`, and on this replay that is
about 61 ms. That is earlier than the chunk's own body, so Chrome did not
have the chunk on the pre-DCL path. A parser-inserted module without
`async` still waits, static imports included. `type=module async` does not.
There is no HAR or `scripts.md` in this tree to name which tag the chunk's
parent was; the ordering above is the one the spec and that Chrome
timestamp both allow.

`held_ms` is how long the body sat ready before the main thread noticed
it. The notice happens while another module is being scanned and, every
64KiB, while `Module::parse` is still reading that module. The child's
imports start then. The scan walks bytes and does not allocate a string
per identifier. `parse_ms` is `Module::parse`. Whatever is left of
`body_ms` to `parsed_ms` after `scan_ms` and `parse_ms` is `jobs_ms`: the
main thread running JavaScript and its microtasks before the loader
returns. It is not an extra sleep after the body was ready, and it is not
the replay server: a HAR entry's `time` is slept once per response.

That `jobs_ms` does not return to the host event loop. `run_jobs` drains
promise jobs and microtasks, then the pump supplies the next body, all
inside the one `load` window hold (`load_url` is `block_on` on the window
thread). A Lit update is one of those microtasks (`await` in
`requestUpdate`), so a chain of them is one task. Chrome ends the task
after that checkpoint and can paint before the next module script. The
chrome UI hook runs during a network wait, not during this JavaScript.
`HIWAVE_CONTENT_THREAD` (the M4 content thread, not in this branch) would
move the hold off the window thread, so chrome could paint while the page
ran. It would not split the page's tasks or paint the content between Lit
microtasks: the content thread would still drain `run_jobs` before the
next yield, and a content frame is produced when that thread returns to
the host. A drain of 50 ms or more logs its census (`hiwave::jobs`, and
on this line when the phase is `jobs`): how many jobs, which kind, and
the callbacks Boa can name. An `await` resume is named `await {function}`
(or `await @{path}` when the function has no name); an empty name used to
be logged as `native`. Host bindings are timed by name, and a DOM write
by its op (`__rustkit_dom_write:setAttr:class`). The window thread drives
one host runtime for the load and for later live turns, so an image
timeout started during the load is still polled on that runtime after the
load returns. What a spec-shaped event loop step would
change, and what M4 already provides, is `docs/event-loop-gap.md`. That
step is not implemented.

When a module's load promise is still pending, or a fetched dependency
never runs, `fetched_scripts` records `reason: "waiting on a dependency"`
and `waiting_on` names that module's own import URLs (a dynamic one is
prefixed `dynamic`). Each URL is cut to 120 characters, so an inline
module's `data:` URL does not fill the field. The name is that module's
edges, not the pending list of a different script. A concat fetched for a
later dynamic `import()` is not recorded as waiting on the inline module.

A static chain (module A imports B, B imports C) still waits for A's body
before B's specifier can be read, and for B's body before C's. It does not
also wait for A's parse. A dynamic chain waits until each `import()` call
runs. Reddit's loader (`SML.load`) builds the next
`www.redditstatic.com/js/concat?…` URL and loads it. A string literal in
the source starts during the parent's parse. A computed specifier starts
at the call, which is as early as the URL exists. The per-origin limit of
6 is a connection slot, not a walk that parses one module before requesting
the next. These fetches do not use the inserted-script queue.

Dynamic `import()` is not a parser-blocking dependency. An unrelated
`fetch()` from the same turn does not hold it.

## Scripts the parser snapshot does not run

**`<template>` contents.** The parser moves a template's children into a
document fragment. They are not in the document tree. The trace records
them with `detail: "template"` and `reason: "never reached"`, and they do
not run. Inserting or cloning the fragment prepares the copy: an inline
classic script runs on that insertion, before the next statement, including
when the destination is a shadow root. The original template entry stays
unrun. A script that is not connected yet (a disconnected shadow, a
fragment) waits until a later insertion connects it. `innerHTML` does not
run scripts. An inline module, once connected, runs as `import()` of a
`data:` URL.

**`<noscript>`.** Scripting is on, so HTML treats the contents as text and
Chrome does not run a script there. This parser still builds the elements
(the tokenizer has no body `noscript` raw-text switch). The runner does not
execute them. The trace records `detail: "noscript"` and
`reason: "never reached"`.

**Declarative shadow trees.** A script in a shadow tree is part of the
document. It runs in shadow-including tree order (the host, then the shadow
tree, then light children) on the same list as a light-tree script of the
same timing. The trace keeps `detail: "shadow tree"`.

**Script-created elements.** `createElement` / `createElementNS` mark a
script fresh. An inline classic script runs synchronously when it is
inserted and connected. An external classic script is force-async unless
`.async = false`. An external module uses dynamic `import()`. A copy runs
only when the original had not started. Moving a parser script does not run
it again.

**After the snapshot.** A script that appears in the tree after the parser
catalog is labeled `inserted after the parser snapshot`. It is not marked
`never reached` at that moment. The trace says `never reached` only if the
script is still unrun when the trace finishes.

## What this does not change

The script budget is JavaScript time only. A fetch uses the 30s network
cap. The budget is not spent while a request is in flight.

Script-inserted external fetches stay on their own queue: at most 8 in
flight, at most 64 queued, cancelled when the navigation is abandoned.
That cap was not raised. Module descendant fetches do not use this queue.
They use the page fetch policy (6 in flight per origin, 24 per page). Three
static imports of one module start together and fit in one origin's 6
slots. A seventh static import to the same origin waits for a connection
slot. That wait is the connection limit, not a walk that parses one module
before requesting the next.

Parser-script fetches are not spawned. They live in the load's script plan.
Dropping the load drops them.

## Host re-entry

The host-stack histogram still counts short function names. The frames
string, the innermost 20, is `name @url:line:col` for a compiled function
and `native {binding} @host` for a native function. A DOM getter's name is
`get Interface.prop`. The style, computed-style, and classList proxies are
`get CSSStyleDeclaration.proxy`, `get CSSStyleDeclaration.computed`, and
`get DOMTokenList.proxy` (and the matching `set` names).

A climb whose names are half `get` and half `(anonymous)` is a JavaScript
getter, or a Proxy `get` trap, re-entering an anonymous function through
native code: the host call is the step between those two frames. The native
side can be a custom-element upgrade inside a getter, an IDL attribute that
dispatches to a userland getter, or a style or layout flush that runs
observers. A page Proxy trap named `get` looks the same until the frames
carry a URL. No headless run reproduced a depth of 256, so this note does
not claim that recursion is fixed. A Mac trace with the longer frames names
the getter.
