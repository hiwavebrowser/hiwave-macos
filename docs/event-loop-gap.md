# Event loop gap

The load is one `block_on` on the window thread (`load_url_blocking` of
`load_url_yielding`), and that future holds `wgate::hold("load")` until it
returns. Inside it, `Context::run_jobs` is one microtask checkpoint: it
drains every promise job, async continuation, and generic job already
queued, including the ones those jobs queue, and it does not return to the
host in between. A drain of 50 ms or more logs a census (`hiwave::jobs`,
and the `module jobs` line): job count, time, kind, and the callbacks Boa
can name (`name @path`). Boa's public code block has the script path and
not the function's start line, so the label has no `:line:col`.

This note is the shape of a spec step, and what the M4 content thread
already provides. It is not an implementation. Atlas decides which of
those to build.

## What the checkpoint was doing wrong

`HostEnqueueTimeoutJob` (Boa's `TimeoutJob`, used by `Atomics.waitAsync`)
and `IntervalJob` were queued on the microtask checkpoint. A timeout was
run on the next `run_jobs`, with its delay ignored. An interval was stored
and never run. A zero-delay timeout is a timer task that is already due,
not a microtask of the script that queued it. Both now wait on a clock and
run from the timer turn (`run_due_timer_jobs`, called at the end of
`run_timers` and `advance_timers`), and that turn then performs one
checkpoint for the reactions they queued. A zero interval runs once per
turn, not in a loop inside the turn.

Nothing else in the audit was the wrong kind. A nested `run_jobs` still
returns immediately, so a job cannot re-enter the checkpoint.

## Kind of each host callback

HTML's event loop runs one task, then a microtask checkpoint, then (when
it is time) a rendering opportunity. A microtask is `queueMicrotask`, a
promise reaction, mutation-observer delivery, or the custom-element
backup queue. Everything else below is a task, on the task source the
spec names, or a rendering callback.

| API | Spec | Here | |
|---|---|---|---|
| `setTimeout` / `setInterval` | timer task source | The page queue in the bindings prelude. The host runs it from `run_timers` (load) or `advance_timers` (live). Not a promise job. | Right kind. One host call runs every timer that is due, including a `setTimeout(0)` queued by a timer that already ran in that call. Their promise reactions run once, when that call's script returns. |
| `MessageChannel` / `window.postMessage` | posted message task | `setTimeout(0)` on that same queue, in post order with other zero-delay timers. | Right kind. React's scheduler yields on a port message, so the yield is a timer task, not a microtask. The same host call still runs the next message before the checkpoint, because the new timer is already due. |
| `requestAnimationFrame` | animation frame callbacks, after the rendering update | Load: a 16 ms timer. Live: one round after the turn's timers. | Not a microtask. There is no rendering update for it to follow. |
| `requestIdleCallback` | idle period after rendering | A `setTimeout(0)` task. It waits while another timer is already due. | A task, not a microtask. Not an idle period after a rendering update. |
| `queueMicrotask` | microtask | `Promise.resolve().then(function queueMicrotask() { … })`. | Right. |
| `MutationObserver` | notify in a microtask | `Promise.resolve().then(notify)`. | Right. |
| Custom element backup queue | microtask when the `[CEReactions]` stack is empty | `queueMicrotask`. The stack itself still runs the element queue on IDL return. | Right. |
| `fetch` / XHR completion | networking task; the fetch promise's reactions are microtasks of that task | `deliver_net_response` is one script. The XHR `load` listener runs in that script. The fetch `then` runs in the checkpoint after it. The script that called `fetch` / `send` does not run either. | Right. |
| Module loader continuation | The host calls `FinishLoadingImportedModule` from the fetch task. `import()` reactions are microtasks. | The load is a Boa async job, polled inside `run_jobs`. Supplying a body wakes the waiter and then checkpoints, so a parent that was awaiting that module resumes there. | The reaction is the right kind. Fusing the parent's continuation into the supply checkpoint is the gap below, not a second kind of queue. |

A scheduler that yields with `MessageChannel` or `setTimeout(0)` does not
get a microtask from us, so it does not spin inside one checkpoint. It can
still run its whole loop inside one timer call: each post queues a
zero-delay timer, and `run_timers` keeps taking timers that are due,
including those just queued. The checkpoint runs only after that call
returns. That is many tasks and then one checkpoint, which is the step
this note does not implement.

## Census of the public Bv chunks

The three chunk files behind
`https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=BvOWmaNPEJ,DZDBx0nCO6,Dyg3q1gB6W`
(358,050 bytes, fetched individually; a multi-chunk concat URL 404s) were
evaluated headlessly as one module. A small `SML.dm` / `SML.di` stub
invokes each factory when its chunk dependencies have finished. Debug,
not a release build:

- `begin_module` checkpoint: 0 jobs. Nothing was waiting on this module.
- `poll_module` checkpoint, after the module body ran: 3 promise jobs,
  61 ms. Two are `bound smlThen` (61 ms together) and one is
  `bound smlFail` (the `DZDBx0nCO6` rejection). No callback from the
  chunk URL itself.
- The factories did register (`BvOWmaNPEJ`, `Dyg3q1gB6W`, `DZDBx0nCO6`).
  `DZDBx0nCO6` rejected with `ReferenceError: CLIENT_CONFIG is not defined`,
  which is a page global this harness does not provide.
- Wall time for parse plus that evaluation was 963 ms on the last
  debug run (978 ms on the run before it). It is not the Mac's 3.1 s.

So the chunk's own factories are not a multi-second microtask drain. The
3.1 s on the Mac is the checkpoint that resumes whoever was already
waiting when Bv's body was supplied (a parent module's `import()`
continuation, or whatever else had queued promise reactions). That
callback's name and script path will be on the next `module jobs` line.
This harness has no such parent, so it cannot name that callback.

## A spec-shaped step, inside today's `block_on`

One step, from the HTML event loop:

1. Take one task from one task source (the timer queue, a posted message,
   a networking completion, a module script that is ready to evaluate).
2. Perform a microtask checkpoint: `run_jobs` once, and do not poll a
   newly queued timer, message, or frame inside it.
3. A rendering opportunity: update the rendering, then run animation frame
   callbacks, then idle callbacks if the event loop is idle. Then the next
   task.

Doing that inside the current `block_on` would stop a `setTimeout(0)` or
`MessageChannel` chain from running to completion before the first
checkpoint, and it would let a module supply return after one checkpoint
instead of after every reaction that supply unblocked. The window thread
would still be inside `block_on`. The chrome pump
(`await_with_chrome_ui`, `CFRunLoopRunInMode`) runs during a network wait,
not during JavaScript, and it pumps the WebKit chrome, not a RustKit
content frame. A rendering opportunity on this thread can build a content
frame only by rendering on the same call stack that is holding the
window. It does not return to tao, so it does not dispatch AppKit input
or draw the chrome. The 15 s `load` hold stays one hold until `block_on`
returns.

The checkpoint itself stays `run_jobs`. The change is what is allowed to
queue work into it: promise reactions, mutation delivery, the custom
element backup queue, and a module-load continuation that is already
ready. Not the next timer, message, frame, or idle callback.

## What M4 already provides

M4 (`HIWAVE_CONTENT_THREAD`, hiwave-macos #730; the threads plan is #691,
`docs/design/THREADS_PLAN_2026-10.md` and `THREAD_OWNERSHIP_2026-10.md`)
moves parse, script, style, layout, and the load's `block_on` onto one
content thread with its own current-thread tokio runtime. The window
thread goes back to AppKit. Chrome input and the presenter can run while
the page is inside a long turn. That removes the 15 s window hold.

It does not add the step above. The content thread would still drain
`run_jobs` to empty before it took the next timer or the next module
body, and a content frame is still produced when that thread next returns
to its host loop. Lit updates that are promise reactions of one supply
would still be one checkpoint, on the content thread instead of the
window thread. The window would be responsive. The page would not be any
fairer, and content would not paint between those microtasks, unless the
content thread itself runs the step.

The two are independent. The step can be added on the window-thread
`block_on` now, or on the content thread after M4, or not at all. M4
alone fixes the frozen window and leaves this checkpoint as it is.
