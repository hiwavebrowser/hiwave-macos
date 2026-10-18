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
waiting on a stylesheet. HiWave loads external stylesheets before the
script walk (the `stylesheet` parser pause). It then awaits each
parser-blocking classic script when the walk reaches it, and records a
`script fetch` pause only for that wait. An inline classic script has no
fetch wait. A later inline script does not wait for an earlier `async`
script.

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
it is not a `script fetch` parser pause.

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

`window` `load` and `readyState` `complete` wait for the parser's `async`
set and for external scripts inserted while the document was loading. Each
inserted script runs and fires `load` or `error` first. A script inserted
from the load handler itself does not hold the event.

A yielding load (`load_url_yielding`) still returns once `DOMContentLoaded`
has fired and an inserted script is outstanding. Live turns run one arrived
body at a time and fire `load` when that chain is idle. Parser `async`
scripts run before that return, after `interactive` is recorded.

## Module graph

"Fetch the descendants of a module script": every static import of a module
is fetched concurrently. The graph is instantiated once those descendants
are present. HiWave does this in one walk:

- siblings discovered together are started together
- when one body arrives it is parsed immediately, and that module's own
  static imports start while its siblings are still in flight

A chain (module A imports B, B imports C) stays serial until each body is
parsed, because the next URL is not known before that. Dynamic `import()`
is not a parser-blocking dependency. The network pump fetches it in the
same wave as `fetch()` / XHR from that turn, so an unrelated request does
not hold the import.

## Scripts the parser snapshot does not run

**`<template>` contents.** The parser moves a template's children into a
document fragment. They are not in the document tree. The trace records
them with `detail: "template"` and they do not run. Inserting the fragment
prepares them: an inline classic script runs on that insertion, before the
next statement. `innerHTML` does not run scripts.

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
