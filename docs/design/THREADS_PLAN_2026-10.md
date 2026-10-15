# Threads plan (2026-10-13, revision 2)

Revision 2 folds in the outside review of 2026-10-13 (Codex): the responsiveness
project now ends at M6 and the shell replacement (M7, M8) is its own project; T0's
guarantee is stated with its limits; the render-thread trigger is a window-stall
measurement, not a percentage; abandonment, scroll correctness and resource ownership
have explicit rules; the schedule is a critical path with review rounds counted.

The decision layer on top of `THREAD_OWNERSHIP_2026-10.md` (the design, by section and
milestone id). That document says how; this one says what we are doing, in what order,
who does it, and what the owner has decided. Inputs: five readers over the design doc,
the code at develop `86e1867b`, the measured costs (Pete's hand-test log of 2026-10-13
and the #687 reviews), Chromium's model with live counts from the Mac, and the platform
constraints in the pinned Boa, wgpu, tao and tokio sources; then four independent
architecture proposals (Chromium-faithful, lowest-risk incremental, page-cost-first,
Phase-X-first). They converge on the same spine; the differences are grafted below.

## 1. What we are deciding

Today one thread, the AppKit main thread, does everything: input, the event loop, the
chrome WebViews' host side, and all of RustKit (fetch, parse, script, cascade, layout,
display list, image decode, GPU upload, render, present). Every slow page operation is
therefore a frozen window. We are deciding to move all page work to threads the window
thread never waits on, in an order where each step lands as one reviewed PR and the
first user-felt change arrives this week. We are also deciding which Chromium roles are
worth copying in a single-process browser, and which are not.

**Two projects, not one.** Project A, *responsiveness*: T0 to M6, including
presenter-owned scroll and the busy bar. It keeps the existing shell (tao, the wry chrome)
and is done when section 10A passes. Project B, *shell replacement* (Phase X steps 2 and
3): M7 and M8, with its own acceptance (10B) and its own schedule, started after Project A
has survived a week of normal use. A constrains B only in this: the input queue (M2) and
the Presenter (M3) are built so the view host can take them over without a rewrite.

## 2. Chrome's threads and ours

Live count on Pete's Mac, 2026-10-13: Chrome 14 processes, 298 threads. Browser process
45, GPU process 24, each renderer 23 to 25, utility processes 14 to 23. In a renderer only
three threads carry semantics; the rest are pool workers and telemetry.

| role | Chrome | HiWave today | HiWave target |
|---|---|---|---|
| UI / window | `CrBrowserMain` (browser process); never blocks on a renderer, every wait has a timeout | the main thread, which also runs every page | main thread: AppKit, input routing, chrome; runs no page code after M4 |
| document: parse, script, style, layout, paint recording | `CrRendererMain`, one per renderer process; run-to-completion JS | the main thread | one `PageThread` for all content at M4, one per live tab at M6; the chrome document on its own at M7 |
| compositing and presenting | `Compositor` (renderer) + `VizCompositorThread` (GPU process): scrolls and animates committed trees while the main thread is busy | inside `relayout` on the main thread (`lib.rs:5260` calls `render`) | the Presenter on the window thread from M3; owns the visible scroll offset from M4b; optional render thread M9 |
| raster and image decode | `ThreadPoolForegroundWorker` x11 per renderer | synchronous on the main thread (`fetch_raster_image`, `lib.rs:566-595`) | a 2-thread decode pool (D1); raster stays in the renderer (wgpu does it) |
| network | network service process + `Chrome_ChildIOThread` | async on the main thread's current-thread tokio runtime; no threads | the page thread's own runtime (M4); a shared net thread only if per-tab runtimes prove costly (optional, after M6) |
| hang detection | `HangWatcher` + the input-ack timer that shows "Page Unresponsive" | none; the 15 s load budget only | the watchdog thread (M1) and the busy bar (M4+) |
| true script threads | Web Workers, own thread each, no DOM access | not linked (`rustkit-worker` is not in the app) | after M6, when a target site needs them |
| process isolation, GPU process, utility processes, Mojo | yes | no | not in scope (owner question 4) |

End state: window + 1 to 8 page threads + chrome page thread + watchdog + decode pool (2)
+ shield + three tickers, about 10 to 18 threads. That is one Chrome renderer's worth,
for the whole browser.

## 3. Two things that stay true

**The DOM is single-threaded.** A document's script, style and layout always share one
thread: script reads the layout it just caused, and the web platform promises
run-to-completion. Chrome does not run page script on a second thread; it keeps the
page's thread off the window thread. "Script threads that interrupt the painter" is not a
shape any browser has. Web Workers are the only script-parallel threads, and they cannot
touch the DOM.

**Threads do not make a page faster; they stop it freezing the window.** On eBay search
(hand test, 2026-10-13) every relayout rebuilt 6,480 boxes: 70 relayouts in two
minutes, median 509 ms, 69 of 70 with `tree_reused=false`, 37 of them from pointer
moves. With a page thread, scrolling and tab switching keep working during those 509 ms;
the page itself still updates at 2 Hz until the page-side work in section 5 lands. The
load-time script cost (8.5 s on eBay search, 15.0 s on eBay home) also stays on the page
thread. Both halves are in the order of work.

## 4. Target thread map and invariants

Threads: **window** (AppKit main), **page-N** (content; one at M4, per live tab at M6),
**ui-page** (the chrome document, M7), **watchdog** (M1), **decode pool** (D1), the
existing shield worker and tickers, an optional **render** thread (M9).

Invariants, each checked against the pinned sources (`Cargo.lock` at develop):

1. Page state is thread-confined, never shared or moved: `Rc<Document>`, `DomBindings`,
   the Boa `Context` and every `LayoutBox` are built on their page thread and die there.
   Boa 0.22's GC heap is per thread (`boa_gc/src/lib.rs:44-51`), so a Context used entirely
   on one spawned thread is supported; nothing requires the main thread.
2. Only the window thread touches AppKit: NSWindow, NSView, tracking areas, menus, modal
   panels (tao `event_loop.rs:168-169`, `window.rs:474-475`; runtime-checked, not typed).
3. wgpu Surface **creation** is window-thread only on Metal (`wgpu-hal metal/surface.rs:
   59-67` panics otherwise). `Surface`, `SurfaceTexture`, `Device`, `Queue`, `Texture` are
   `Send + Sync` (`wgpu/src/api/surface.rs:188`, `device.rs:22`, `queue.rs:17`), and
   configure, acquire and present carry no thread check, so a render thread is possible
   later; off-main, `current_extent` is `None`, so extents must be passed explicitly.
   Apple-side behaviour of off-main present is unexercised in this repo: M9 proves it.
4. A frame reaches the screen only as an immutable commit: `FrameCommit { generation,
   nav_id (from M3b; `nav_generation` until then), Arc<DisplayList>, content size, max scroll, ... }`. `DisplayList` is
   `Send + Sync`. The Presenter draws the newest commit per view and nothing else.
5. Input reaches a page in arrival order, tagged by view, with a sequence number (M2).
   Wheel events are also applied by the Presenter (M4b) so scrolling never waits.
6. A runaway script is stopped, never paused, and never from the window thread's own
   stack: host-call refusal (T0, `rustkit-js/src/lib.rs:481-504`) and the VM interrupt
   (M1, the Boa patch) read a per-view interrupt word that the watchdog or the user's Stop
   sets. Boa has no cross-thread interrupt today (`vm/runtime_limits.rs:3-14`): the patch
   is required, there is no way around it.
7. Every page thread runs its message loop **inside** `block_on` of its own current-thread
   tokio runtime; spawned h2 drivers (`rustkit-http/src/lib.rs:518-527`) only progress
   there. A page thread that leaves `block_on` stalls its own HTTP/2. Staying inside is
   necessary, not sufficient: a long synchronous script or layout turn also stops that
   runtime, so the page's own network makes no progress during it. That is measured at M4
   (h2 pings and timeouts across a 5 s turn); if it fails, the shared net thread moves from
   optional to before M5.
8. Every cross-thread message is plain data or an `Arc` of an immutable object. No
   `Rc`, no reference, no `Arc` to page state. This keeps a later process split a
   transport change (owner question 4). It does not make a process split *only* a
   transport change: shared `Arc` caches, GPU resources, crash recovery and resource
   ownership would each need their own design.
9. **The Presenter is still on the window thread until M9**, so the window can still
   stall on renderer execution, image upload, surface acquire and present. This is
   measured, not assumed away: from M3 every window-thread hold of 50 ms or more is logged
   with its stage (the stall log #666 adds), and section 5's W-gate decides M9.
10. **Abandonment is containment, not recovery.** A page thread stuck in one native call
    is detached after 2 s without an acknowledgement; it keeps its CPU and memory until it
    returns. Rules: every abandoned thread is tracked with what it retains; its view's
    epoch is bumped so no late message, commit, decode result or network callback from it
    can reach a live document; at most two abandoned threads may exist, and a third
    abandonment shows "HiWave needs to restart to recover" instead of spawning another;
    the busy bar says a restart reclaims the resources.
11. **A retained frame owns what it draws.** A `FrameCommit` holds `Arc`s to every image,
    glyph run and SVG it references, so it stays drawable after navigation replaces the
    page state; resources are released when the last commit that names them is dropped.

## 5. Order of work

Sizes: S under 300 lines, M about a lane-week, L two. "Where": **lane** = the headless Z
lane on the Mac; **cloud** = a Linux cloud session (no screen); **Atlas-Mac** = the real
window, run by Atlas when Pete says the Mac is free. Ids in parentheses are the design
doc's; "(new)" marks a step the design doc does not have. The plan's T-ids are its own: the
design doc's T1/T2 (host-call refusal, VM interrupt) are this plan's T0 and M1.

| # | scope | user task it makes pass | size | where | after |
|---|---|---|---|---|---|
| T0 (M0) | **PR #666 (landed 2026-10-14)**: a 5 s budget per live turn and per input event, on script that makes host calls. A temporary mitigation with stated limits (below), not a responsiveness guarantee. Adds two tests first: one click whose three handlers each spin, and an interval that overruns every time | a post-load runaway that calls the host (h20, the eBay `forEach` spin) is cut off after 5 s per turn instead of never | S, written | Atlas-Mac rerun of the corrected `i0a` checks, then merge | Pete's decision 1 |
| T1 | **Relayout stage timer**: `build / layout / display_list / snapshot / render` ms on every `relayout cause=` line, from the spans that already exist (`lib.rs:5036, 5045, 5121, 5240, 5260`); coalesce pointer moves per drain | we learn which stage owns eBay's 509 ms; a fast sweep across listings costs one relayout per drain, not one per move | S | cloud | none |
| T2 (new) | **Scroll paints before the page hears it**: `scroll_view` updates the offset and re-presents the last display list; the relayout follows | once the window thread processes a wheel event, the page moves on that frame instead of after the relayout it triggers. It cannot help a wheel event that arrives while the thread is already blocked; M4b fixes that | S | lane; Atlas-Mac check | none |
| T3 | **Phase X step 1, narrowed**: delete only the `webview-fallback` feature (system WebKit drawing *content* on macOS; `hiwave-app/Cargo.toml:28`, 23 cfg branches). The non-macOS wry content path (`main.rs:1220`) is **not** touched: Windows and Linux keep it | nothing felt; M4 then has one macOS content path to convert | S | cloud | none |
| P1 | **#687**: paint-only hover takes the short path (held on the unpainted focus change) | hovering a cnn or eBay listing no longer rebuilds the page | M, in review | lane | none |
| P2 | **Lazy geometry snapshot**: build the JS-visible box map on first read, not on every relayout (35 ms of every relayout on the author's Mac) | every relayout on every site drops ~35 ms | M | cloud | T1 |
| P3 | **Render without clones**: no `commands.clone()` for image upload (`lib.rs:14110`) or scroll (`:14177`); scroll as a renderer offset | wheel on the Wikipedia portal re-presents without copying 2.2M commands | M | cloud, Atlas-Mac check | T1 |
| D1 | **Decode pool**: two `rustkit-decode` threads behind a bounded queue (64 jobs; when full the page thread decodes inline). A job carries the document's `nav_generation` (today's counter; `nav_id` replaces it at M3b); a result for a superseded navigation is dropped. The page thread never waits on a result: layout uses the placeholder size and relays out on arrival, as `image_loaded` does today | image decode never stalls script or layout; eBay's 84 and cnn's 41 images decode in parallel | S | cloud | none |
| M1 | **Boa VM interrupt + `ThreadStamp` / `ViewControl` + watchdog thread**: vendor `boa_engine` 0.22.0 with the ~30-line patch; delete the stale, unused `boa_gc` / `boa_parser` patches (`Cargo.lock` lists them `[[patch.unused]]`) | a pure spin with no host calls (h20b, `while(true){}`) stops within budget + 250 ms | S-M | cloud | T0 |
| M2 | **Ordered, view-tagged input queue** replacing the three statics in `rustkit-viewhost/src/macos.rs` | scroll-then-click and type-then-click arrive in order; input is tagged with the view it was delivered to | M | lane | none |
| M3 | **Engine split**: `PageEngine` + `Presenter` on one thread, in two PRs (pure move, then `FrameCommit`). Includes the **W-gate log**: every window-thread hold of 50 ms or more, by stage, and input-to-present latency | nothing felt: h1-h16 and the 26-case campaign identical; it is the price of M4 | L | lane | P3 |
| M3b | **Cooperative load cancel**: `nav_id`, `cancel_nav`, the check between scripts and subresource phases | engine test: a load with a spinning script and twenty slow subresources is cancelled within 200 ms | M | cloud | M1 |
| **M4** | **One content thread** behind `HIWAVE_CONTENT_THREAD=1`, default after one hand-test day: `RustKitView` becomes a `PageHandle`; the loop runs inside the page runtime's `block_on` | **the responsiveness milestone**: URL bar, chrome clicks, window drag and tab switch answer within 100 ms while h19 spins or eBay relayouts; Stop ends a spin within 1 s | L (protocol PR, then thread PR) | lane; Atlas-Mac | M1, M2, M3, M3b, T3 |
| M4b | **Presenter-owned scroll** (the doc's optional M9b, promoted), root scroller only; rules and cases below | with h20 spinning in the front tab a wheel flick moves the last frame within one frame, and a click after the flick lands on what the user saw | M | lane; Atlas-Mac | M4 |
| M4c | **Busy bar + native accelerators**: "this page is busy — Stop" after 2 s of one turn; Esc / Cmd+. / Cmd+W on the window thread. Stop policy changes here (section 7, decision 1) | the user sees why a page is slow and can stop it without a beachball | S | lane | M4 |
| R1 | **Resource ownership audit**, before a second live document exists: web-font slot per document; image ids, glyph caches, the SVG cache, resource callbacks and frame dependencies each get a named owner and a release rule (invariant 11) | nothing felt; two live documents cannot overwrite each other's fonts or images | M | cloud | M3 |
| M5 | **One retained view per tab** | clicking tab 1 while tab 2 spins shows tab 1's last frame within 100 ms; switching tabs no longer reloads; a flick in tab A never lands in tab B | M | lane; Atlas-Mac | M4b, R1 |
| M6 | **One page thread per tab**, cap active + 7 most recent and a memory budget (owner question 3); abandonment rules of invariant 10. **Project A ends here** | tab 1 keeps ticking while tab 2 spins; a new tab loads during the spin | L | lane; Atlas-Mac | M5, M4c |
| M7 | *Project B.* **Chrome document as a page thread** (Phase X step 2), `HIWAVE_RUSTKIT_CHROME=1` | all of the above through a RustKit-drawn tab strip; a chrome script forced to spin does not stop Cmd+W | L, two PRs | lane; Atlas-Mac | M4c, M6 |
| M8 | *Project B.* **`ViewHostMac` replaces tao** (Phase X step 3) | tasks 1-7 on a build with no tao and no wry | L | lane; Atlas-Mac | M2, M7 |
| M9 | Render thread: renderer execute, image upload, acquire and present off the window thread (Surface creation stays on it). **Triggered by the W-gate**, not by a share of a relayout | no window-thread hold over 50 ms and input-to-present under 32 ms at p95 on eBay search, cnn and the Wikipedia portal | M | lane; Atlas-Mac | W-gate at M4; before M5 if it fails |

Lanes in parallel from day one: cloud takes T1, T3, P2, P3, D1, M1, M3b (none needs a
screen); the lane takes T2, P1, M2, then M3 and M4. Atlas runs the real-window checks
when the Mac is free. Pete hand-tests each landed step in the weekly build.

**T0's limits, as #666 states them.** The budget is per event and per turn, not per user
action: one click is `mousedown`, `mouseup` and `click`, each with its own 5 s, so up to
about 15 s. An interval that overruns every time is stopped every time, so it holds the
window 5 s out of every 5 and starves the page's other timers. Script with no host calls
is not stopped at all until M1. The parity campaign runs with interruption off, so it
says nothing about the budget. T0 is worth landing; it bounds the common eBay-style spin.
It does not make the window responsive. M4 does.

**The W-gate.** At M4, with the content thread on, read the window-stall log on eBay
search, cnn and the Wikipedia portal. If any stage holds the window 50 ms or more, or
input-to-present exceeds 32 ms at p95, M9 moves ahead of M5. A percentage of a relayout is
not the test: 20% of a second is a freeze and 60% of 2 ms is nothing.

**What a scroll frame does (M4b).** Today a scroll re-executes the whole display list at
a new offset, which is proportional to the page: 2.2M commands on the Wikipedia portal. P3
removes the copies, not the execution. M4b's scroll frame must be bounded by the viewport,
not the page: the Presenter keeps the last rendered frame as a texture taller than the
viewport and a scroll frame translates and blits it, re-executing only when the retained
band is exhausted or a new commit arrives. If the W-gate shows the re-execute path is
already under 8 ms on all three sites, the retained band is deferred.

**M4b correctness rules.** Sequence order alone is not enough; these are acceptance cases:

- *Flick then click.* A click carries the Presenter's visible offset and the commit
  generation the user saw. The page thread applies that offset before hit-testing, so the
  click lands on what was on screen, even if `max_scroll` changed in between.
- *Fixed and sticky.* Fixed boxes are a separate layer in the commit (the engine already
  holds them apart, #681) and are not translated. Sticky boxes are translated with the
  page and clamped by the bounds the commit carries.
- *Nested scrollers.* Not predicted at first: a wheel over a nested scroll container goes
  to the page as today. Listed as a known limit.
- *`scrollTo()` during a flick.* The commit's `scroll_ack` names the last wheel sequence
  the page applied; a commit with a script-set offset replaces the Presenter's offset and
  drops deltas at or below that sequence.
- *Resize or navigation with wheel events outstanding.* Unacknowledged deltas are dropped
  on a new `nav_id` and re-clamped on resize.

## 5b. Schedule: the critical path

The earlier "four lane-weeks to M4" was a target, not a forecast. By this plan's own
sizes M3 (two PRs) and M4 (two PRs) are each two lane-weeks and sequential: four weeks of
writing alone, before M2, review, integration and Mac time.

| step | writing | review rounds (this week's PRs took 3 to 5, about a day each) | Mac checks | running total |
|---|---|---|---|---|
| M2 input queue | 1 week, lane | 3 to 5 days | half a day | week 2 |
| M3 PR1: pure move | 1 week, lane (after M2) | 3 to 5 days | campaign A/B | week 3-4 |
| M3 PR2: FrameCommit + W-gate log | 1 week | 3 to 5 days | half a day | week 5 |
| M1, M3b cancel, D1, P2, P3, T1, T3 | cloud, in parallel | their own rounds | none | not on the path if started now |
| R1 resource audit | cloud, after M3 PR2 | 3 to 5 days | none | before M5; off the path if started at week 5 |
| M4 PR1: protocol | 1 week | 3 to 5 days | h1-h16 | week 6-7 |
| M4 PR2: thread, flag on | 1 week | 3 to 5 days | a hand-test day, W-gate read | week 8 |

Forecast: **M4 default-on in about eight weeks**, six if review rounds halve; M4b, M4c,
M5 and M6 add four to six more. The one lane is the constraint. A second Mac-capable
worker on M2 while the lane does M3 PR1 would take two weeks off.

What the page-side PRs buy, from the measurements: tree reuse alone took one eBay
relayout from 509 ms to 337 ms, so build and cascade are at least 170 ms of the 509;
the snapshot is ~35 ms; on the author's Mac render was 143 of 218 ms for 5,704 boxes.
T1 replaces these bounds with a split per site.

## 6. First felt win

This week, two modest things: T0 bounds the common post-load spin at 5 s per turn (with
the limits above), and T2 makes a processed wheel event move the page on that frame.
Neither makes the window responsive. The first step that does is M4, about eight weeks
out on the critical path in 5b; P1 (#687) lands sooner and removes most of the
half-second hovers on its own.

## 7. Decisions (approved)

**Approved by Pete, 2026-10-13: "You have a go on this plan."** All six recommendations
below stand as decided. Release rule that goes with it: no weekly release until this plan
is completed; the next release is the weekend release after Project A's acceptance (10A)
passes. Develop keeps moving and Pete keeps hand-testing it.

1. **Stop policy.** Now (T0): 5 s automatic per turn, with the limits stated. From M4c:
   the front tab is never killed automatically while the busy bar is showing; the user's
   Stop is immediate. A background tab keeps the 5 s automatic stop, and any tab is
   stopped at 30 s as a backstop. Recommended: yes to all three.
2. **Vendor a patched Boa 0.22 (M1).** Recommended: yes. The patch is small but its risk
   is not measured in lines: it lands with tests for the interrupt race, for unwinding out
   of nested calls and jobs, and for the context being usable afterwards.
3. **How many tabs keep a live page thread (M6).** Recommended: active + 7 as the first
   setting, plus a memory budget that discards the least recent tab sooner on a small
   machine.
4. **Process per tab.** Recommended: no for this project, with the bounded abandonment
   rules of invariant 10.
5. **Shell replacement (M7, M8) as its own project after M6.** Recommended: yes. When it
   runs, the chrome goes on its own page thread, not the window thread.
6. **Promote presenter-owned scroll (M4b) to right after M4.** Recommended: yes, with the
   hit-testing and positioned-content cases as acceptance.

## 8. What this does not solve

- The page's own cost: after P1-P3 an eBay hover is still roughly 250-300 ms of page
  time, and script-triggered relayouts still block the page's own JS. Incremental restyle
  and layout beyond #687 is a separate plan.
- Time to interactive: 8.5 s and 15 s of load-time script stay on the page thread.
- A single long native call (a pathological regex, a 5 s layout) cannot be interrupted;
  its thread is abandoned after 2 s, not reclaimed.
- Crash, memory and site isolation: one process; a panic on any thread is caught at the
  thread boundary at best, a segfault takes the app.
- Windows and Linux content: stays on wry until a host lane lands there.
- Web Workers, service workers, IME and drag-and-drop on the RustKit content view.
- Nested scroll containers do not get presenter-owned scroll in Project A.
- A page's own network stalls while its thread runs a long turn, unless the net thread is
  promoted (invariant 7).
- An abandoned thread's CPU and memory are not reclaimed without a restart.

## 9. Risks and how each is retired

| risk | retired by |
|---|---|
| The 509 ms split is unknown, so P1-P3 are ranked on bounds | T1 lands first and is read before P2/P3 are prioritised |
| M3 is the largest change: a 36k-line `lib.rs` where render, `create_view`, `set_view_bounds` and image upload assume the engine owns the compositor | two PRs, the first a pure move with the campaign identical A/B; compile-time `assert_send` on every type that crosses |
| Off-main present on Metal is unexercised | M4 keeps present on the window thread; M9 lands behind a flag with a resize-tearing check in the real window |
| The Presenter stalls the window on a heavy page | the W-gate at M4 measures it and moves M9 forward |
| Presenter scroll and page layout disagree: wrong-target clicks, drifting fixed headers | the five M4b cases are real-window acceptance checks |
| Abandoned threads pile up | capped at two, epoch-fenced, restart prompt (invariant 10) |
| Per-page tokio runtimes stall h2 if the loop leaves `block_on` | invariant 7, with an engine test that holds a request across a long turn |
| Process-global resources (the web-font slot at `rustkit-text/src/webfonts.rs:284-290`, image ids, glyph and SVG caches) break with two live documents | R1, before M5 |
| The vendored Boa drifts from upstream | one file, one patch, a test that fails if the hook is missing; upstream PR filed |
| Watchdog kills a legitimate 6 s handler | the busy bar at 2 s (M4c) lets the user decide; the automatic kill goes away with it |
| The real-window driver's input path is unproven on a granted seat | it now is: 13 checks, 142 assertions, passed on the Mac 2026-10-13 |

## 10. Acceptance

By hand and by `tools/real_window`, in the built app on macOS.

### 10A. Project A, responsiveness (done at M6)

- [ ] Task 1: tab 2 spins; tab 1 shows within 100 ms and keeps ticking.
- [ ] Task 2: Stop, Esc and Cmd+. end a spin within 1 s; the page stays usable.
- [ ] Task 3: Cmd+W closes a spinning tab within 1 s.
- [ ] Task 4: during a spinning load, typing in the URL bar, opening a tab and dragging the window answer within 100 ms.
- [ ] Task 5: on a pure `while(true){}` spin, the user's Stop takes effect within 250 ms; a background tab is stopped automatically at 5 s and any tab at 30 s.
- [ ] Task 6: a page the server holds for 20 s does not freeze tab switch or the shelf.
- [ ] Task 7: input arrives in order and in the right tab.
- [ ] Task 8: h1-h16 pass; the 26-case campaign is identical A/B; Wikipedia, YouTube and eBay look as before.
- [ ] One click whose three handlers each spin, and an interval that always overruns: the window answers within 100 ms throughout.
- [ ] Scroll: a flick during a 509 ms relayout moves the page within one frame; a click right after it hits what was on screen; a fixed header stays put; `scrollTo()` mid-flick wins.
- [ ] W-gate: no window-thread hold of 50 ms or more, input-to-present under 32 ms at p95, on eBay search, cnn and the Wikipedia portal.
- [ ] A third abandoned thread produces the restart prompt, not a fourth thread; no late result from an abandoned page reaches a live document.

### 10B. Project B, shell replacement (M7, M8)

- [ ] Task 9: every 10A item passes with the RustKit chrome and no tao or wry in the macOS build.
- [ ] A chrome script forced to spin does not stop Cmd+W or page scrolling.
- [ ] IME composition, clipboard and drag-in work on the RustKit views.

## 11. Diagnostics

The Inspector's diagnostic snapshot (landed in #674) gains a `threads` block as the
milestones land: stage timings per relayout (T1), input queue depth and oldest event age
(M2), frame age and commit generation per view (M3), `nav_id` and `nav_epoch` (M3b), the
interrupt reason and who set it (M1), window-thread holds (M3), abandoned threads (M6).
It is read by the real-window checks and shown in the Inspector page; no milestone waits
on the Inspector.
