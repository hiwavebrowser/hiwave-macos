# Script scheduling contract

Draft 1, Z lane, 2026-10-19 (Friday rule 1 of the Z plan, Codex's process
review: one behavior, one contract, one owner). The "script scheduling"
behavior covers the 0-scripts budget regression fix (#720), the Reddit
inserted-script rework (#716 / #727), and Chrono24's first load. They are
one behavior. This is its contract: what the engine promises the page for
each kind of script, so that a change to any of the three is reviewed
against one text and not against the other two PRs.

It starts from `docs/loading-rules.md` and `docs/event-loop-gap.md` (#735,
`cloud/loader-followups`), which describe the lists and the event-loop
shape; this document adds the lifecycle table per script kind, the budget
and cancellation rules, and the watchdog rule. Where a line is not yet
true on develop it says so and names the branch that makes it true.

Spec references are the ones `loading-rules.md` lists: prepare the script
element, execute the script element, the parser's script end tag, "the
end", fetch the descendants of a module script, delaying the load event.

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
  only, since #720: a network wait is not charged (section 5).
- **Stop.** The watchdog setting the view's interrupt word for the turn,
  or the host-call deadline refusing a call. The stopped script cannot
  catch it; the turn gets `STOP_GRACE` (50 ms) for the reactions its
  finished script queued; that grace stop is final.
- **Network cap.** `SCRIPT_NETWORK_CAP` = 30 s per script fetch (#720). It
  fails that fetch. It never arms the watchdog and never refuses a host
  call. A body that arrived still runs.

## 2. The lifecycle per script kind

Each kind: what starts it, the states it can be in, what completes it,
what cancels it, and what the page observes. "Observes" means the events
on the element, the order against other scripts, and `document.readyState`
at the time it runs.

### 2.1 Parser classic, inline

- **Starts:** the script walk reaches the element, after stylesheets and
  fonts (the `stylesheet` and `font` parser pauses). No fetch.
- **States:** pending (not reached) -> running -> ran | threw | stopped |
  skipped.
- **Completes:** `evaluate_named` returns, microtask checkpoint included.
- **Cancels:** navigation cancel between scripts (the rest of the walk does
  not run); the budget already spent before it starts (`OverBudget`, not
  started); the engine poisoned by a JS panic (`Skipped`). A spin inside it
  is stopped by the watchdog at the remaining budget.
- **Observes:** runs in document order, parser-blocking; `readyState` is
  `loading`; `document.currentScript` is the element; no `load`/`error`
  event (inline). Errors that escape reach `window` `error` and the
  script log as `Threw`.

### 2.2 Parser classic, external, no `async`/`defer`

- **Starts:** the fetch starts with the load's script plan; the walk awaits
  it when it reaches the element (`script fetch` parser pause). The body
  then runs as 2.1.
- **States:** fetching -> fetched | fetch failed -> running -> ran | threw
  | stopped | skipped.
- **Completes:** as 2.1.
- **Cancels:** the network cap fails the fetch (`FetchFailed(Transport)`),
  the element hears `error`; a non-2xx status is `FetchFailed(Status)`,
  `error`, and Chromium does not run it either; a body over
  `MAX_PAGE_SCRIPT_BYTES` (4 MB) is `Skipped`; dropping the load drops the
  fetch (parser fetches are not spawned).
- **Observes:** document order with 2.1; `readyState` `loading`; `load`
  on the element after it ran, `error` if it could not be fetched. It does
  not wait for an earlier `async` script or for images.

### 2.3 Parser `defer`, and parser `type=module` without `async`

- **Starts:** the fetch starts during the stylesheet load; the body (for a
  module, the graph: every static import fetched concurrently, section 2.6)
  is awaited at the deferred list, after the parser-blocking list.
- **States:** fetching -> ready -> running -> ran | threw | stopped;
  module: fetching graph -> instantiating -> evaluating (a top-level
  `await` keeps it evaluating) -> done | failed.
- **Completes:** classic as 2.1; module when `poll_module` reports `Done`
  or `Evaluating` (the element hears `load` then), `Threw` (`load`, the
  throw is logged), `Failed` or `Loading` (`error`).
- **Cancels:** navigation; budget spent before it starts (`OverBudget`);
  a module whose fetch's JavaScript (not the wait) spent the budget is not
  evaluated (`OverBudget`); the network cap on any fetch of the graph.
- **Observes:** tree order, after every parser-blocking script, before
  `DOMContentLoaded`; `readyState` `loading` while they run;
  `DOMContentLoaded` waits for every one of them. A deferred module's
  fetch is not a parser pause.

### 2.4 Parser `async` (classic external, or `type=module async`)

- **Starts:** the fetch starts with the plan. The body runs from the
  "as soon as possible" set, after `DOMContentLoaded` has been recorded
  and before `load`, even when the body arrived earlier (allowed by the
  spec; keeps the order stable).
- **States:** as 2.2.
- **Completes / cancels:** as 2.2 and 2.3.
- **Observes:** does not block the parser, other parser scripts, or
  `DOMContentLoaded`; a slow fetch delays `load`; `readyState` is
  `interactive` when it runs; `load` / `error` on the element.

### 2.5 Script-inserted external classic script

- **Starts:** insertion into a connected tree with a `src` present
  prepares it: the fetch goes on the inserted-script queue (at most 8 in
  flight, 64 queued; dropped when the navigation is abandoned). Default
  force-async; `.async = false` puts it on the in-order list.
- **States:** queued -> fetching -> arrived -> running -> ran | threw |
  stopped; or fetch failed; or not started.
- **Completes:** `pump_inserted_scripts` runs an arrived body inside the
  current turn (load or live) and fires `load`; a failed fetch fires
  `error`. In-order scripts run in insertion order; a failure does not
  hold the next one.
- **Cancels:** the network cap (`error`); the budget spent before the body
  runs: on develop the body is dropped (`OverBudget`, no `load`/`error`)
  and on `cloud/reddit-feed-live` (#716) / `cloud/yielding-load-tail`
  (#727) it is carried to a later live turn instead; navigation.
- **Observes:** `readyState` as of the turn it runs in (`interactive`
  during the load's pump, `complete` on a live turn); does not delay
  `DOMContentLoaded`; delays `load` while the document is loading (an
  inserted script runs and fires its event before `load`; one inserted
  from the `load` handler itself does not hold it). Under the yielding
  load (#727) `load_url` returns after `DOMContentLoaded` with the script
  still outstanding, and `load` fires from the live turn when that chain
  is idle.
- **Open (Chrono24, Atlas 2026-10-16 15:30):** an element inserted with no
  `src` and no text is not "already started"; setting `src` later must
  prepare and fetch it, and an insertion made inside a timer or `load`
  callback must still be pumped. Fixtures for both orders are owed; this
  line is the contract, not a claim about develop.

### 2.6 Script-inserted module, and dynamic `import()`

- **Starts:** an inserted `type=module` with `src` runs as a dynamic
  `import()`; an inline inserted module as `import()` of a `data:` URL.
  A static import's fetch starts as soon as the specifier is known (the
  pre-parse scan); a computed `import(url)` when the call runs. Module
  fetches use the page fetch policy (6 per origin, 24 per page), not the
  inserted-script queue.
- **States:** requested -> body -> scanned -> parsed -> linked ->
  evaluated; `waiting on a dependency` while any import is out.
- **Completes:** the graph root's `link` and `evaluate`; the `import()`
  promise settles in the checkpoint after the supply that unblocked it.
- **Cancels:** the network cap on any body (`Failed`, the promise rejects);
  the budget: `pump_modules` is skipped for the rest of a turn whose
  budget is gone, and the modules are asked for again on the next turn;
  navigation.
- **Observes:** a dynamic `import()` is never a parser-blocking
  dependency and never holds `DOMContentLoaded`; the pump before
  `DOMContentLoaded` fetches only a parser module's static imports; an
  `import()` an earlier script queued is answered after
  `DOMContentLoaded` with `fetch()` and XHR.

### 2.7 Not scripts, and scripts the parser does not run

`nomodule` (skipped by design, Chromium too); non-JavaScript `type` (not a
script); `<template>` contents (run only when a copy is inserted and
connected); `<noscript>` contents (built by this parser, never run);
`innerHTML` (runs nothing); a copy of a script that already started (does
not run); a moved parser script (does not run again). These are as
`loading-rules.md` has them and are not changed by this contract.

## 3. Lifecycle events and `readyState`

| Step | `readyState` set to | Waits for | Does not wait for |
|---|---|---|---|
| parser scripts | `loading` | 2.1, 2.2 in order; then the deferred list 2.3 | async, inserted, fetch/XHR, images |
| `DOMContentLoaded` (document, then window) | `interactive` (set just before) | the above | async, inserted, fetch/XHR, images, dynamic `import()` |
| network pump | `interactive` | one round per `script_network_rounds`: deliveries, imports, inserted scripts | nothing is awaited past the network cap |
| `load` (window) | `complete` (set just before) | parser `async` set, images still out, inserted scripts outstanding while loading | a script inserted from the `load` handler |
| load timers | `complete` | `run_timers` to `timer_horizon_ms` (5 s virtual), `MAX_TIMER_CALLBACKS` | |

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
microtask checkpoint, and not a step that starts after the budget is
spent.**

Concretely:

- `with_script_js` arms the remaining allowance of the installed budget
  for the execution, and disarms before the next network wait. Executions
  do not nest; a nested call joins the outer turn.
- A spent budget stops JavaScript; it never takes the watchdog away. An
  execution that starts with no allowance left is armed with `STOP_GRACE`
  (#720 at `7883d82a`; on develop and on `dc75279d` it ran unarmed).
  Lifecycle steps still run after the budget is spent; only a script not
  yet started is skipped.
- Input-event listeners arm `live_turn_budget_ms` per event
  (`arm_live_turn`); there is no network wait inside one.
- The microtask checkpoint is part of the execution that queued it, so
  it runs under that execution's turn. A nested `run_jobs` returns at
  once; the outer loop runs the new jobs.
- A stop moves the rest of the turn to the grace turn (`STOP_GRACE`), with
  the host deadline moved with it (#720 `9613fb91`). The grace stop is
  final.
- The watchdog is a thread; a turn published while it sleeps a tick wakes
  it when the turn's budget ends sooner (`begin_turn`). That wake is
  tested by a mutation-checked test (#726 at `ea99dca0`).

What this rule does not promise: a wall-clock bound on the whole load (the
network waits are outside the budget, section 5), and a stop for script
that the vendored Boa cannot interrupt between two back-edges (none known
since M1's patch).

## 5. Budget and network cap

- The budget counts JavaScript execution only (#720). A fetch, a module
  body, an inserted-script fetch, an XHR or `fetch()` awaited during the
  load: none of it is charged.
- Each script fetch has the 30 s network cap. Expiry fails that request
  (`FetchFailed(Transport("timed out"))` for scripts, a rejected promise
  for a module, a network error for XHR/`fetch()`), never a stop.
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
| Navigation cancelled between scripts | the rest of the walk, the pump and the steps do not run; parser fetches drop with the load; inserted-script fetches are cancelled when the navigation is abandoned | nothing more fires on the old document |
| Stop (user, `stop_all`) | every turn of the view is stopped until a new document clears the word | the running script cannot catch it; later turns of the same document do not run script |
| Budget spent | the running execution is stopped; later executions on the path are armed with the grace (section 4); unstarted parser scripts are `OverBudget` | `console.warn("Script stopped: it ran past N ms (source)")`; an uncatchable error in the stopped frame; the Inspector's `script_error` |
| Network cap | that fetch fails | `error` on the element, a rejected import, a network error |
| JS engine panic | nothing more runs on the page | `EngineFailed(Panic)` in the log |

## 7. Ordering summary

1. Parser-blocking classic scripts, document order (2.1, 2.2).
2. The deferred list in tree order (2.3), modules included.
3. `readyState` `interactive`; `DOMContentLoaded` on document, then window.
4. The network pump: deliveries, dynamic imports, inserted scripts, in
   arrival order per round; the parser `async` set (2.4).
5. `readyState` `complete`; `load` on window (after outstanding inserted
   scripts while loading; under #727, from the live turn once idle).
6. The load's timers, then further pump rounds for what they asked.
7. Live turns: timers first (what they ask is answered this turn), then
   deliveries while budget remains, then modules, then inserted scripts.

## 8. What this contract changes in review

A PR touching any of: the script walk, `pump`/`pump_all`,
`pump_inserted_scripts`, `pump_modules`, `with_script_js`, the budget or
cap constants, the lifecycle steps, `pump_live`, or the yielding load,
must say in its body which lines of sections 2 to 6 it changes, and carry
a fixture for each changed line. A regression against a line here is a
regression on develop (Friday rule 3: fixed, flagged off, or reverted
within a working day of being localised).

## 9. Open items this contract records, not resolves

- Chrono24: `src` set after insertion, insertion from a timer or `load`
  callback (2.5).
- The spec-shaped event-loop step (one task, one checkpoint, a rendering
  opportunity) is described in `event-loop-gap.md` and not implemented;
  M4 moves the hold off the window thread and does not add the step.
- The chrome UI turn during network waits is #739 (split from #720) and
  needs a design for tao's handler re-entrancy before it returns.
- The six known shapes of `loading-rules.md`'s "scripts the parser does
  not run" have no WPT pass count yet (Friday rule 5).
