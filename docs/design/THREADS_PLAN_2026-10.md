# Threads plan (2026-10-13)

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
+ shield + three tickers, about 13 to 20 threads. That is one Chrome renderer's worth,
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
   nav_id, Arc<DisplayList>, content size, max scroll, ... }`. `DisplayList` is
   `Send + Sync`. The Presenter draws the newest commit per view and nothing else.
5. Input reaches a page in arrival order, tagged by view, with a sequence number (M2).
   Wheel events are also applied by the Presenter (M4b) so scrolling never waits.
6. A runaway script is stopped, never paused, and never from the window thread's own
   stack: host-call refusal (T1, `rustkit-js/src/lib.rs:481-504`) and the VM interrupt
   (T2, the Boa patch) read a per-view interrupt word that the watchdog or the user's Stop
   sets. Boa has no cross-thread interrupt today (`vm/runtime_limits.rs:3-14`): the patch
   is required, there is no way around it.
7. Every page thread runs its message loop **inside** `block_on` of its own current-thread
   tokio runtime; spawned h2 drivers (`rustkit-http/src/lib.rs:518-527`) only progress
   there. A page thread that leaves `block_on` stalls its own HTTP/2.
8. Every cross-thread message is plain data or an `Arc` of an immutable object. No
   `Rc`, no reference, no `Arc` to page state. This keeps a later process split a
   transport change (owner question 4).

## 5. Order of work

Sizes: S under 300 lines, M about a lane-week, L two. "Where": **lane** = the headless Z
lane on the Mac; **cloud** = a Linux cloud session (no screen); **Atlas-Mac** = the real
window, run by Atlas when Pete says the Mac is free. Ids in parentheses are the design
doc's.

| # | scope | user task it makes pass | size | where | after |
|---|---|---|---|---|---|
| T0 (M0) | **Land PR #666**: 5 s budget on every live turn, input dispatch and `execute_script` | a post-load runaway (h20, the eBay `forEach` spin) frees the window within 5 s instead of never | S, written | Atlas-Mac rerun of the corrected `i0a` checks, then merge | Pete's decision 1 |
| T1 | **Relayout stage timer**: `build / layout / display_list / snapshot / render` ms on every `relayout cause=` line, from the spans that already exist (`lib.rs:5036, 5045, 5121, 5240, 5260`); coalesce pointer moves per drain | we learn which stage owns eBay's 509 ms; a fast sweep across listings costs one relayout per drain, not one per move | S | cloud | none |
| T2 (M0b) | **Scroll paints before the page hears it**: `scroll_view` updates the offset and re-presents the last commit; the relayout follows | a wheel tick on eBay search moves the page on the next frame, not after 509 ms | S | lane; Atlas-Mac check | none |
| T3 | **Phase X step 1**: delete `webview-fallback` and the non-macOS wry content path | nothing felt; removes 23 cfg branches M4 would otherwise have to carry | S | cloud | none |
| P1 | **#687**: paint-only hover takes the short path (held on the unpainted focus change) | hovering a cnn or eBay listing no longer rebuilds the page | M, in review | lane | none |
| P2 | **Lazy geometry snapshot**: build the JS-visible box map on first read, not on every relayout (35 ms of every relayout on the author's Mac) | every relayout on every site drops ~35 ms | M | cloud | T1 |
| P3 | **Render without clones**: no `commands.clone()` for image upload (`lib.rs:14110`) or scroll (`:14177`); scroll as a renderer offset | wheel on the Wikipedia portal re-presents without copying 2.2M commands | M | cloud, Atlas-Mac check | T1 |
| D1 | **Decode pool**: two `rustkit-decode` threads; `fetch_raster_image` sends bytes, gets `Arc<LoadedImage>` back as a message | image decode never stalls script or layout; eBay's 84 and cnn's 41 images decode in parallel | S | cloud | none |
| M1 | **Boa VM interrupt + `ThreadStamp` / `ViewControl` + watchdog thread**: vendor `boa_engine` 0.22.0 with the ~30-line patch; delete the stale, unused `boa_gc` / `boa_parser` patches (`Cargo.lock` lists them `[[patch.unused]]`) | a pure spin with no host calls (h20b, `while(true){}`) stops within budget + 250 ms | S-M | cloud | T0 |
| M2 | **Ordered, view-tagged input queue** replacing the three statics in `rustkit-viewhost/src/macos.rs` | scroll-then-click and type-then-click arrive in order; a flick in tab A never lands in tab B | M | lane | none |
| M3 | **Engine split**: `PageEngine` + `Presenter` on one thread, in two PRs (pure move, then `FrameCommit`) | nothing felt: h1-h16 and the 26-case campaign identical; it is the price of M4 | L | lane | P3 |
| M3b | **Cooperative load cancel**: `nav_id`, `cancel_nav`, the check between scripts and subresource phases | engine test: a load with a spinning script and twenty slow subresources is cancelled within 200 ms | M | cloud | M1 |
| **M4** | **One content thread** behind `HIWAVE_CONTENT_THREAD=1`, default after one hand-test day: `RustKitView` becomes a `PageHandle`; the loop runs inside the page runtime's `block_on` | **the responsiveness milestone**: URL bar, chrome clicks, window drag and tab switch answer within 100 ms while h19 spins or eBay relayouts; Stop ends a spin within 1 s | L (protocol PR, then thread PR) | lane; Atlas-Mac | M1, M2, M3, M3b, T3 |
| M4b | **Presenter-owned scroll** (the doc's optional M9b, promoted): the Presenter draws committed offset + unacknowledged wheel deltas; commits carry `scroll_ack` | with h20 spinning in the front tab a wheel flick moves the last frame within one frame | M | lane; Atlas-Mac | M4 |
| M4c | **Busy bar + native accelerators**: "this page is busy — Stop" after 2 s of one turn; Esc / Cmd+. / Cmd+W on the window thread | the user sees why a page is slow and can stop it without a beachball | S | lane | M4 |
| M5 | **One retained view per tab** | clicking tab 1 while tab 2 spins shows tab 1's last frame within 100 ms; switching tabs no longer reloads | M | lane; Atlas-Mac | M4b |
| M6 | **One page thread per tab**, cap active + 7 most recent (owner question 3) | tab 1 keeps ticking while tab 2 spins; a new tab loads during the spin | L | lane; Atlas-Mac | M5 |
| M7 | **Chrome document as a page thread** (Phase X step 2), `HIWAVE_RUSTKIT_CHROME=1` | all of the above through a RustKit-drawn tab strip; a chrome script forced to spin does not stop Cmd+W | L, two PRs | lane; Atlas-Mac | M4c, M6 |
| M8 | **`ViewHostMac` replaces tao** (Phase X step 3) | tasks 1-7 on a build with no tao and no wry | L | lane; Atlas-Mac | M2, M7 |
| M9 | Optional render thread; only if T1's `render` share stays above 40% of a hover relayout after P3 | Wikipedia portal wheel-to-present under one frame | M | lane; Atlas-Mac | M6 |

Lanes in parallel from day one: cloud takes T1, T3, P2, P3, D1, M1, M3b (none needs a
screen); the lane takes T2, P1, M2, then M3 and M4. Atlas runs the real-window checks
when the Mac is free. Pete hand-tests each landed step in the weekly build.

What the page-side PRs buy, from the measurements: tree reuse alone took one eBay
relayout from 509 ms to 337 ms, so build and cascade are at least 170 ms of the 509;
the snapshot is ~35 ms; on the author's Mac render was 143 of 218 ms for 5,704 boxes.
T1 replaces these bounds with a split per site.

## 6. First felt win

This week, two things: T0 turns an unbounded freeze on a runaway page into a 5 s one
(the branch exists; it needs Pete's decision and the real-window rerun), and T2 makes
the wheel move the page on the next frame even on eBay search. The responsiveness
milestone, M4, is four lane-weeks out if M1, M2, M3 and M3b run in parallel as above;
six if they run in series.

## 7. Decisions needed from Pete

1. **5 s live-turn budget (T0).** Recommended: yes. It is temporary; from M4c the busy
   bar replaces the automatic kill with the user's own Stop.
2. **Vendor a patched Boa 0.22 (M1).** Recommended: yes; it is the only way to stop a
   pure spin, and the patch is 30 lines with a test. Offer it upstream.
3. **How many tabs keep a live page thread (M6).** Recommended: active + 7, folded into
   Tab Decay.
4. **Process per tab.** Recommended: no, not now; invariant 8 keeps the door open.
5. **Chrome on its own page thread (M7).** Recommended: yes; the window thread then never
   runs script, which is what the watchdog relies on.
6. **Promote presenter-owned scroll (M4b) from optional to right after M4.** Recommended:
   yes; it is the single Chrome property users feel most after "the app did not freeze".

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

## 9. Risks and how each is retired

| risk | retired by |
|---|---|
| The 509 ms split is unknown, so P1-P3 are ranked on bounds | T1 lands first and is read before P2/P3 are prioritised |
| M3 is the largest change: a 36k-line `lib.rs` where render, `create_view`, `set_view_bounds` and image upload assume the engine owns the compositor | two PRs, the first a pure move with the campaign identical A/B; compile-time `assert_send` on every type that crosses |
| Off-main present on Metal is unexercised | M9 is optional and last; M4 keeps present on the window thread |
| Per-page tokio runtimes stall h2 if the loop leaves `block_on` | invariant 7, with an engine test that holds a request across a long turn |
| The process-global web-font slot (`rustkit-text/src/webfonts.rs:284-290`) breaks with two live documents | made per-document in M5's first PR |
| The vendored Boa drifts from upstream | one file, one patch, a test that fails if the hook is missing; upstream PR filed |
| Watchdog kills a legitimate 6 s handler | the busy bar at 2 s (M4c) lets the user decide; the automatic kill goes away with it |
| The real-window driver's input path is unproven on a granted seat | it now is: 13 checks, 142 assertions, passed on the Mac 2026-10-13 |

## 10. Acceptance

The plan is done when, in the built app on macOS, by hand and by `tools/real_window`:

- [ ] Task 1: tab 2 spins; tab 1 shows within 100 ms and keeps ticking.
- [ ] Task 2: Stop, Esc and Cmd+. end a spin within 1 s; the page stays usable.
- [ ] Task 3: Cmd+W closes a spinning tab within 1 s.
- [ ] Task 4: during a spinning load, typing in the URL bar, opening a tab and dragging the window answer within 100 ms.
- [ ] Task 5: a pure `while(true){}` spin is stopped within budget + 250 ms.
- [ ] Task 6: a page the server holds for 20 s does not freeze tab switch or the shelf.
- [ ] Task 7: input arrives in order and in the right tab.
- [ ] Task 8: h1-h16 pass; the 26-case campaign is identical A/B; Wikipedia, YouTube and eBay look as before.
- [ ] Task 9: tasks 1-7 pass with the RustKit chrome and no tao in the build.
- [ ] Scrolling never waits on a page: a flick during a 509 ms relayout moves the page within one frame.
