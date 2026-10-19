# Script scheduling contract

Draft 2, Z lane, 2026-10-19 (Friday rule 1 of the Z plan, Codex's process
review: one behavior, one contract, one owner). Draft 1 was `c3fe9f7f`;
draft 2 answers Prometheus's R1 HOLD on it (five spec mismatches, five
imprecisions), checked against the WHATWG HTML Living Standard and against
develop `004b316f` (#720 merged) and the weekend stack head #737
`74623f49`.

The "script scheduling" behavior covers the 0-scripts budget regression fix
(#720, now on develop), the Reddit inserted-script rework (#716 / #727),
and Chrono24's first load. They are one behavior. This is its contract:
what the engine promises the page for each kind of script, so that a
change to any of the three is reviewed against one text and not against
the other two PRs.

It starts from `docs/loading-rules.md` and `docs/event-loop-gap.md` (#735,
`cloud/loader-followups`), which describe the lists and the event-loop
shape; this document adds the lifecycle table per script kind, the budget
and cancellation rules, and the watchdog rule.

**Reading rule.** Sections 2 to 7 say what the spec says, in the engine's
terms. Where develop (or the stack) does something else, the line says
`[deviation Dn]` and section 9 names the deviation, its owner and the
fixture that pins it. A deviation is never fixed by rewording the spec
line. A deviation that is a product choice says so and stays a deviation.

Spec references are the ones `loading-rules.md` lists: prepare the script
element, execute the script element, the parser's script end tag, "the
end", fetch the descendants of a module script, delaying the load event;
plus "update the current document readiness", "unload a document", "stop
document loading" and "abort a document" for sections 3 and 6.

## 1. Terms

- **Load path.** `load_url`: parse, stylesheets and fonts, the script walk,
  `DOMContentLoaded`, the network pump, `load`, the load's timers. One
  `block_on` on the window thread (until M4).
- **Live path.** `pump_live` turns after `load_url` returns: timers,
  network deliveries, inserted scripts, module fetches; and input-event
  listeners, each its own turn.
- **Execution.** One piece of JavaScript the host starts: a script body,
  a listener, a timer callback, a network delivery, a module evaluation
  step, and the microtask checkpoint that `evaluate` drains before it
  returns. Executions never nest across a network wait.
- **Turn.** What the watchdog sees: an execution with a budget, published
  on the thread's `ThreadStamp` (`begin_script_turn` / `end_script_turn`).
- **Budget.** The load's `script_budget_ms` (app 15 s, parity-capture 5 s)
  and the live turn's `live_turn_budget_ms` (app 5 s). JavaScript time
  only, since #720 (develop `004b316f`): a network wait is not charged
  (section 5).
- **Stop (watchdog).** The watchdog setting the view's interrupt word for
  the turn, or the host-call deadline refusing a call. The stopped script
  cannot catch it; the turn gets `STOP_GRACE` (50 ms) for the reactions
  its finished script queued; that grace stop is final.
- **Stop (user).** The toolbar's Stop, `Engine::stop`: the spec's "stop
  document loading" (section 6). Not the same thing as a watchdog stop.
- **Network cap.** `SCRIPT_NETWORK_CAP` = 30 s per script fetch (#720). It
  fails that fetch. It never arms the watchdog and never refuses a host
  call. A body that arrived still runs.

## 2. The lifecycle per script kind

Each kind: what starts it, the states it can be in, what completes it,
what cancels it, and what the page observes. "Observes" means the events
on the element, the order against other scripts, and `document.readyState`
at the time it runs.

### 2.1 Parser classic, inline

- **Starts:** the script walk reaches the element. The spec holds a
  parser-inserted classic script only while the document has a style
  sheet that is blocking scripts (an inline one only at script nesting
  level 1 or less). Fonts never block a script in the spec; the engine's
  `font` parser pause before the walk is an engine choice, not a promise
  to the page, and a PR may remove it without touching this contract.
  `defer` and `async` on an inline classic script do nothing: it runs
  here.
- **States:** pending (not reached) -> running -> ran | threw | stopped |
  skipped.
- **Completes:** `evaluate_named` returns, microtask checkpoint included.
- **Cancels:** navigation cancel between scripts (the rest of the walk does
  not run); the budget already spent before it starts (`OverBudget`, not
  started); the engine poisoned by a JS panic (`Skipped`). A spin inside it
  is stopped by the watchdog at the remaining budget.
- **Observes:** runs in document order, parser-blocking; `readyState` is
  `loading`; `document.currentScript` is the element; no `load`/`error`
  event (only an external script fires `load`). Errors that escape reach
  `window` `error` and the script log as `Threw`.

### 2.2 Parser classic, external, no `async`/`defer`

- **Starts:** the fetch starts with the load's script plan; the walk awaits
  it when it reaches the element (`script fetch` parser pause). The body
  then runs as 2.1.
- **States:** fetching -> fetched | fetch failed -> running -> ran | threw
  | stopped | skipped.
- **Completes:** as 2.1.
- **Cancels and failures:** `src=""` queues `error` and fetches nothing
  `[deviation D6]`; an unparseable `src` is `Skipped`; the network cap
  fails the fetch (`FetchFailed(Transport)`) and the element hears
  `error`; a non-2xx status is `FetchFailed(Status)`, `error`, and
  Chromium does not run it either; a body that is not UTF-8 is a transport
  failure; a body over `MAX_PAGE_SCRIPT_BYTES` (4 MB) is `Skipped`;
  dropping the load drops the fetch (parser fetches are not spawned). The
  response's MIME type does not fail a classic script unless the response
  carries `X-Content-Type-Options: nosniff`, which the engine does not
  read yet (section 9, not a deviation: a page cannot observe the
  difference unless it serves a non-script under `nosniff`).
- **Observes:** document order with 2.1; `readyState` `loading`; `load`
  on the element after it ran, `error` if it could not be fetched. It does
  not wait for an earlier `async` script or for images.

### 2.3 Parser `defer`, and parser `type=module` without `async`

- **Starts:** the fetch starts during the stylesheet load; the body (for a
  module, the graph: every static import fetched concurrently, section 2.6)
  is awaited at the deferred list, after the parser-blocking list. An
  inline `type=module` is on the same list, in tree order, with no fetch
  of its own; its base URL is the document base URL.
- **States:** fetching -> ready -> running -> ran | threw | stopped;
  module: fetching graph -> instantiating -> evaluating (a top-level
  `await` keeps it evaluating) -> done | failed.
- **Completes:** classic as 2.1; module when `poll_module` reports `Done`
  or `Evaluating` (an external module's element hears `load` then; an
  inline module's element hears nothing), `Threw` (`load` for an external
  one, the throw reported to `window` `error`), `Failed` or `Loading`
  (`error`).
- **Cancels and failures:** navigation; budget spent before it starts
  (`OverBudget`); a module whose fetch's JavaScript (not the wait) spent
  the budget is not evaluated (`OverBudget`); the network cap on any fetch
  of the graph. A module response that is not 2xx or not a JavaScript
  MIME type is a null result: `error` on the element, nothing runs
  (develop: `Denial::BadMime` in `rustkit-net` `policy.rs`).
- **Observes:** tree order, after every parser-blocking script, before
  `DOMContentLoaded`; `DOMContentLoaded` waits for every one of them. The
  spec sets `readyState` to `interactive` (and fires `readystatechange`)
  BEFORE the deferred list runs ("the end", step 3 before step 5), so a
  deferred or parser-module script reads `interactive` `[deviation D1]`.
  `document.currentScript` is `null` in a module. A deferred module's
  fetch is not a parser pause.

### 2.4 Parser `async` (classic external, or `type=module async`)

- **Starts:** the fetch starts with the plan. The spec runs an `async`
  script when it is ready, from the set of scripts that will execute as
  soon as possible; that can be while the parser is still running
  (`readyState` `loading`), between deferred scripts, or after
  `DOMContentLoaded`. The engine runs the set after `DOMContentLoaded` has
  been recorded and before `load`, even when the body arrived earlier.
  The page cannot tell that from a slower fetch, which is why it is not a
  deviation; it is also why the contract does not promise a page which
  `readyState` it sees in an `async` script.
- **States:** as 2.2.
- **Completes / cancels:** as 2.2 and 2.3.
- **Observes:** does not block the parser, other parser scripts, or
  `DOMContentLoaded`; a slow fetch delays `load`; `load` / `error` on
  the element.

### 2.5 Script-inserted external classic script

- **Starts:** the spec prepares a non-parser-inserted script when it
  becomes connected, when its children change while connected, or when
  `src` is set while connected. An element with no `src` and no text
  returns from "prepare" BEFORE `already started` is set, so setting
  `src` on it later prepares it (the Chrono24 shape, section 9). The fetch
  goes on the inserted-script queue (at most 8 in flight, 64 queued;
  dropped when the navigation is abandoned). The force-async flag starts
  true: the default is the "as soon as possible" set; `.async = false`
  clears it and puts the script on the in-order list (develop `004b316f`
  has the queue but not the flag; the flag is on #727 `74623f49`).
- **States:** queued -> fetching -> arrived -> running -> ran | threw |
  stopped; or fetch failed; or not started.
- **Completes:** `pump_inserted_scripts` runs an arrived body inside the
  current turn (load or live) and fires `load`; a failed fetch fires
  `error`. In-order scripts run in insertion order; a failure does not
  hold the next one.
- **Cancels and failures:** `src=""` queues `error` and fetches nothing
  (develop: `web_scripts.js` `prepare`); the network cap (`error`); the
  budget spent before the body runs: on develop the body is dropped
  (`OverBudget`, no `load`/`error`) and on `cloud/reddit-feed-live`
  (#716) / `cloud/yielding-load-tail` (#727) it is carried to a later live
  turn instead; navigation. MIME as 2.2.
- **Observes:** `readyState` as of the turn it runs in (`interactive`
  during the load's pump, `complete` on a live turn); does not delay
  `DOMContentLoaded`; delays `load` while the document is loading (an
  inserted script runs and fires its event before `load`; one inserted
  from the `load` handler itself does not hold it). Under the yielding
  load (#727) `load_url` returns after `DOMContentLoaded` with the script
  still outstanding, and `load` fires from the live turn when that chain
  is idle.

### 2.6 Script-inserted module, and dynamic `import()`

A module `<script>` that script inserts is prepared and executed as a
script element (prepare the script element; execute the script element).
It is not a dynamic `import()`, and the contract pins the three places
where that shows:

- **Base URL.** An inline inserted module is created with the document
  base URL: `import './x.js'` inside it resolves against the document, and
  `import.meta.url` is the document URL. An external one has its own URL.
  `[deviation D3]`
- **Element events and error reporting.** An external module element hears
  `load` after its graph ran, or `error` when the fetch (any node of the
  graph) produced a null result. An evaluation error is reported to
  `window` `error` (and the script log), not delivered as a rejected
  promise or `unhandledrejection`, and the element still hears `load`
  (the module ran). An inline module element hears neither. `[deviation
  D3]`
- **Ordering.** An inserted module with `async = false` goes on the
  in-order list with the classic ones of 2.5; the default (force-async)
  goes on the "as soon as possible" set. `[deviation D3]`
- **Starts:** a static import's fetch starts as soon as the specifier is
  known (the pre-parse scan); a computed `import(url)` when the call runs.
  Module fetches use the page fetch policy (6 per origin, 24 per page),
  not the inserted-script queue.
- **States:** requested -> body -> scanned -> parsed -> linked ->
  evaluated; `waiting on a dependency` while any import is out.
- **Completes:** the graph root's `link` and `evaluate`; an `import()`
  promise settles in the checkpoint after the supply that unblocked it.
- **Cancels and failures:** the network cap on any body (`Failed`: `error`
  on the element, or a rejected `import()` promise); a non-2xx or
  non-JavaScript MIME response is a null result (same); the budget:
  `pump_modules` is skipped for the rest of a turn whose budget is gone,
  and the modules are asked for again on the next turn; navigation.
- **Observes:** a dynamic `import()` is never a parser-blocking
  dependency and never holds `DOMContentLoaded`; the pump before
  `DOMContentLoaded` fetches only a parser module's static imports; an
  `import()` an earlier script queued is answered after
  `DOMContentLoaded` with `fetch()` and XHR.

Implementation note (not contract): the engine may keep running an
external inserted module through its `import()` machinery as long as the
element events, the error reporting, the ordering and the base URL above
are what the page sees.

### 2.7 Not scripts, and scripts the parser does not run

`nomodule` on a classic script (skipped by design, Chromium too; ignored
on a module script); non-JavaScript `type` (a data block, not a script);
`<template>` contents (run only when a copy is inserted and connected);
`<noscript>` contents (built by this parser, never run); `innerHTML` (runs
nothing); a copy of a script that already started (does not run: cloning
copies `already started`); a moved parser script (does not run again).
These are as `loading-rules.md` has them and are not changed by this
contract.

## 3. Lifecycle events and `readyState`

| Step | `readyState` set to | Waits for | Does not wait for |
|---|---|---|---|
| parser scripts | `loading` | 2.1, 2.2 in order | async, inserted, fetch/XHR, images |
| end of parsing | `interactive` (fires `readystatechange`) `[D1]` | the parser | the deferred list |
| deferred list | `interactive` | 2.3 in tree order | async, inserted |
| `DOMContentLoaded` | `interactive` | the deferred list | async, inserted, fetch/XHR, images, dynamic `import()` |
| network pump | `interactive` | one round per `script_network_rounds`: deliveries, imports, inserted scripts | nothing is awaited past the network cap |
| `load` (window) | `complete` (set in the same task, just before) | parser `async` set, images still out, inserted scripts outstanding while loading | a script inserted from the `load` handler |
| load timers | `complete` | `run_timers` to `timer_horizon_ms` (5 s virtual), `MAX_TIMER_CALLBACKS` | |

`DOMContentLoaded` is ONE event, dispatched at the `Document` with
`bubbles: true` and `cancelable: false` ("the end", step 6). A `window`
listener sees it in the bubble phase with `event.target === document`; a
capture listener on `window` runs before the document's listeners; a
document listener's `stopPropagation()` keeps it from `window`.
`[deviation D2]`

`load` is one event at `window` (not bubbling); `readyState` is
`complete` when its listeners run and `readystatechange` has fired.

A lifecycle step is one execution (section 4). A step that is stopped is
logged as `event:DOMContentLoaded`, `event:load` or `timers` with
`OverBudget`; a clean step is not a script-log record.

## 4. The watchdog arms on every JavaScript entry point

This is the rule #720's R1 found missing, and Handtest's CNN freeze. It
is the one line of this contract that no other document states.

**Every execution the host starts runs inside an armed turn when
`interrupt_scripts_at_budget` is on. No entry point runs JavaScript with
no deadline and no turn: not a lifecycle step, not a timer, not a
listener, not a network delivery, not a module supplied late, not the
microtask checkpoint, not an unload-time listener (section 6), and not a
step that starts after the budget is spent.**

Concretely:

- `with_script_js` arms the remaining allowance of the installed budget
  for the execution, and disarms before the next network wait. Executions
  do not nest; a nested call joins the outer turn.
- A spent budget stops JavaScript; it never takes the watchdog away. An
  execution that starts with no allowance left is armed with `STOP_GRACE`
  (develop `004b316f` through #720; on `dc75279d` and before it ran
  unarmed). Lifecycle steps still run after the budget is spent; only a
  script not yet started is skipped.
- Input-event listeners arm `live_turn_budget_ms` per event
  (`arm_live_turn`); there is no network wait inside one.
- The microtask checkpoint is part of the execution that queued it, so
  it runs under that execution's turn ("clean up after running script").
  A nested `run_jobs` returns at once ("performing a microtask
  checkpoint" guard); the outer loop runs the new jobs.
- A stop moves the rest of the turn to the grace turn (`STOP_GRACE`), with
  the host deadline moved with it (#720 `9613fb91`). The grace stop is
  final.
- The watchdog is a thread; a turn published while it sleeps a tick wakes
  it when the turn's budget ends sooner (`begin_turn`). That wake is
  tested by a mutation-checked test (develop `004b316f`, the commit #726
  carried; #726 itself is now the timing tests only).

What this rule does not promise: a wall-clock bound on the whole load (the
network waits are outside the budget, section 5), and a stop for script
that the vendored Boa cannot interrupt between two back-edges (none known
since M1's patch).

## 5. Budget and network cap

Engine policy; the spec has nothing here to contradict.

- The budget counts JavaScript execution only (#720). A fetch, a module
  body, an inserted-script fetch, an XHR or `fetch()` awaited during the
  load: none of it is charged.
- Each script fetch has the 30 s network cap. Expiry fails that request
  (`FetchFailed(Transport("timed out"))` for scripts, a null result for a
  module, a network error for XHR/`fetch()`), never a stop.
- A delivery that arrives with less than `STOP_GRACE` of budget left is
  shelved and parked for the next live turn instead of being resumed under
  a stop (#720). Parked deliveries keep their take order. Nothing waiting
  is dropped: a delivery is dropped only when no callback is pending for
  it.
- The headline budgets do not move without Pete: load 15 s, live 5 s,
  parity-capture 5 s.
- The live loop never waits for a request (`LIVE_NETWORK_SLICE` 2 ms per
  turn); a request started on a live turn is delivered on a later one.

## 6. Cancellation

| Cause | What happens | What the page observes |
|---|---|---|
| Navigation away (the new document commits) | the old document is unloaded: `pagehide` (`persisted: false`), `visibilitychange` if it was visible, then `unload` fire on it, each listener under an armed live turn; then nothing more runs on it: its timers, deliveries, inserted scripts and module fetches are dropped `[deviation D4]` | the three events, then silence; a listener that spins is stopped like any other |
| Navigation cancelled before commit | the new load's walk, pump and steps do not run; its parser fetches drop; its inserted-script fetches are cancelled when the navigation is abandoned | the shown document notices nothing: its script keeps running |
| Stop (user), spec | "stop document loading": the parser is aborted, pending fetches are cancelled (scripts still out hear `error`), `load` fires for what did arrive; timers, listeners and running script are NOT stopped | the page keeps running |
| Stop (user), engine `[deviation D5, deliberate]` | during a load of a committed document, `Engine::stop` cancels the navigation and `stop_all` sets the view's word: every turn of that document is stopped until a new document clears it; on an idle view Stop changes nothing | the running script cannot catch it; later turns of the same document do not run script |
| `window.stop()` from page script | the spec's stop document loading only; it must NOT trip D5 or a page that calls `stop()` kills itself (develop does not bind `window.stop`; when it does, this line holds) | as the spec row |
| Budget spent | the running execution is stopped; later executions on the path are armed with the grace (section 4); unstarted parser scripts are `OverBudget` | `console.warn("Script stopped: it ran past N ms (source)")`; an uncatchable error in the stopped frame; the Inspector's `script_error` |
| Network cap | that fetch fails | `error` on the element, a rejected import, a network error |
| JS engine panic | nothing more runs on the page | `EngineFailed(Panic)` in the log |

## 7. Ordering summary

1. Parser-blocking classic scripts, document order (2.1, 2.2).
2. End of parsing: `readyState` `interactive`, `readystatechange`.
3. The deferred list in tree order (2.3), modules included.
4. `DOMContentLoaded`: one bubbling event at the document (section 3).
5. The network pump: deliveries, dynamic imports, inserted scripts, in
   arrival order per round (in-order scripts in insertion order); the
   parser `async` set (2.4).
6. `readyState` `complete`; `load` on window (after outstanding inserted
   scripts while loading; under #727, from the live turn once idle).
7. The load's timers, then further pump rounds for what they asked.
8. Live turns: timers first (what they ask is answered this turn), then
   deliveries while budget remains, then modules, then inserted scripts.

## 8. What this contract changes in review

A PR touching any of: the script walk, `pump`/`pump_all`,
`pump_inserted_scripts`, `pump_modules`, `with_script_js`,
`web_scripts.js`, the budget or cap constants, the lifecycle steps,
`pump_live`, `Engine::stop`, or the yielding load, must say in its body
which lines of sections 2 to 7 it changes, and carry a fixture for each
changed line. A PR that closes a deviation in section 9 says which one and
carries that deviation's fixture. A regression against a spec line here is
a regression on develop (Friday rule 3: fixed, flagged off, or reverted
within a working day of being localised). A reviewer who finds a spec line
wrong asks for the fixture in the PR that fixes it, not for a softer
sentence.

## 9. Deviations and open items

Each deviation: where develop (`004b316f`) and the stack head (#737
`74623f49`) differ from sections 2 to 7, who owns it, and the fixture that
pins the spec behaviour. Ownership follows Friday rule 1: the Z lane owns
the integrated result and the fixtures; the Grok seats implement and
measure the pieces that live in the inserted-script rework (#716 / #727).
Nothing here starts new page-lifecycle work on its own (Friday rule 2:
two behavioral changes in flight); each item lands with the PR that
already owns its code, or waits.

| id | Deviation | Where | Owner | Fixture |
|---|---|---|---|---|
| D1 | `readyState` becomes `interactive` after the deferred list, inside the `event:DOMContentLoaded` step; a `defer` or parser-module script reads `loading`. Spec: set at the end of parsing, before the deferred list, with `readystatechange`. | `rustkit-engine/src/lib.rs` `set_ready_state("interactive")` at 5040 (develop), 6821 (#737) | Z lane (fix with the lifecycle-step change for D2) | a `defer` script and an inline module that log `document.readyState`, plus a `readystatechange` listener added by an earlier parser script; expected `interactive` logged before either runs |
| D2 | `DOMContentLoaded` is dispatched twice, at `document` then at `window`, each non-bubbling with `target` the dispatch target (`__rustkit_fire`, `bindings/src/lib.rs` 427, `bubbles: false`). Spec: one bubbling event at the document. | `lib.rs` 5042 and 5045 (develop), 6824 and 6830 (#737) | Z lane | a window listener asserting `event.target === document` and `eventPhase === BUBBLING_PHASE`; a window capture listener that must run before the document's; a document listener calling `stopPropagation()` after which the window listener must not run; the window `load` row unchanged |
| D3 | An external inserted module runs through `import(url)`: an evaluation error becomes `error` on the element (spec: `load`, with the error reported to `window`); no in-order list for modules (`async = false` ignored). An inline inserted module does not run at all (`web_scripts.js` header: "Not here (stated): inline module scripts"). Draft 1's "runs as `import()` of a `data:` URL" described neither develop nor the spec. | `web_scripts.js` `prepare`, module arms (develop 61 to 68; #737 same shape with the force-async flag) | Grok seats (#716 / #727 rework); Z lane reviews against 2.6 | an inline inserted module with `import './dep.js'` and `import.meta.url` logged (document base); an external inserted module that throws (window `error` fires, element `load` fires); two inserted modules with `async = false` and the slower one first (run in insertion order) |
| D4 | Nothing fires on the old document when a navigation commits: `rustkit-core` `lifecycle.rs` models `pagehide` but the engine never calls it; `unload` and `visibilitychange` are not fired either. Spec: unload a document. | `rustkit-engine/src/lib.rs` navigation commit; `rustkit-core/src/lifecycle.rs` 234 | Atlas to assign (new lifecycle behavior; under Friday rule 2 it waits for a slot). Until then a deviation, not a bug to fix in passing. | a page that sets `sessionStorage` from `pagehide` and `unload`, then a same-origin navigation; the new page reads both keys; a spinning `unload` listener is stopped at the live budget and the navigation still commits |
| D5 | User Stop during a load of a committed document is a kill switch for that document's script (`Engine::stop` -> `cancel_nav` -> `load_cancelled` -> `control.stop_all`, `lib.rs` 11418 to 11523 on develop). Spec: Stop cancels fetches and the parser only. **Deliberate** (a stopped page that keeps spinning is what Stop is for in this product). | `lib.rs` 11523 (develop) | Atlas (records it; Pete's call to change) | the existing `stop_all_stops_every_turn_until_a_new_document_clears_it`; plus, when `window.stop` is bound: a page whose script calls `window.stop()` and then sets a timer that must still fire |
| D6 | A parser script with `src=""` is joined onto the base URL and fetched (the document itself, as a script). Spec: queue `error`, no fetch. The inserted path already does the spec (`web_scripts.js` 60). | `lib.rs` 4441 (develop) | Z lane (one arm in `fetch_page_scripts`) | `<script src=""></script>` with an `error` listener; expected one `error`, zero requests for the document URL as a script |

Not deviations, recorded so they are not re-litigated:

- `nosniff` is not read, so a classic script served as a non-script under
  `X-Content-Type-Options: nosniff` runs here and not in Chromium. One
  arm in the classic fetch when a page needs it; no page on the board
  does.
- The `font` parser pause before the script walk (2.1) and the `async`
  set running after `DOMContentLoaded` (2.4) are engine choices a page
  cannot observe.

Open items this contract records, not resolves:

- Chrono24: develop prepares an inserted script when `src` is set on a
  connected element (`web_scripts.js` `changed`) and when it is inserted
  from a timer or `load` callback (the live pump); the fixtures for both
  orders are still owed, and the live Chrono24 symptom (no request for the
  challenge script) is not yet explained by a fixture. Rule stands: run
  the page's own scripts like Chrome; no code that recognises a challenge.
- The spec-shaped event-loop step (one task, one checkpoint, a rendering
  opportunity) is described in `event-loop-gap.md` and not implemented;
  M4 moves the hold off the window thread and does not add the step.
- The chrome UI turn during network waits is #739 (split from #720) and
  needs a design for tao's handler re-entrancy before it returns.
- The six known shapes of `loading-rules.md`'s "scripts the parser does
  not run" have no WPT pass count yet (Friday rule 5); the D1 to D3
  fixtures should be taken from `html/semantics/scripting-1/the-script-element`
  where one exists, so the pass count and the fixture are the same file.
