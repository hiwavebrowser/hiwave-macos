# Thread ownership: who owns the window thread (W7-E)

Status: design only, 2026-10-09, against `develop` at `f54103d`. No code changes with it.
Scope: one design that covers two planned changes, because both move thread ownership:
**responsiveness** (page script must not freeze the browser) and **Phase X independence**
(remove `webview-fallback`, draw the chrome in RustKit, then `rustkit-viewhost` owns the macOS window and tao leaves macOS, then wry leaves).

Paths are relative to the repository root. Line numbers are for `f54103d`.
"Window thread" means the thread that runs `event_loop.run` (`crates/hiwave-app/src/main.rs:2040`), which on macOS is the AppKit main thread.

---

## 1. Today: what runs on which thread

### 1.1 Threads that exist

| thread | created at | does |
|---|---|---|
| window (main) thread | `main.rs:591` `fn main`, loop at `main.rs:2040` | everything below |
| init sync ticker | `main.rs:1992` | sleeps 500 ms, sends `UserEvent::Sync*` through the proxy |
| decay ticker | `main.rs:2008` | `UserEvent::DecayTick` every 60 s |
| focus checker | `main.rs:2018` | `UserEvent::FocusAutoTriggerCheck` every 5 s |
| shield worker | `crates/hiwave-app/src/shield_adapter.rs:305-333` | owns the `AdBlocker`; answers filter queries over mpsc |
| WebKit's own processes | (system) | the chrome, shelf and inspector WKWebViews' content |

There is no rayon and no multi-thread tokio runtime in `hiwave-app`. tokio is `rt` + `time` only (`crates/hiwave-app/Cargo.toml:73`). `rustkit-net` creates no threads or runtimes. `rustkit-http`'s async client creates none either; its blocking `ClientBuilder::build` builds a current-thread runtime (`crates/rustkit-http/src/lib.rs:1281`), and nothing outside `rustkit-http` uses that blocking client (grep for `rustkit_http::blocking`). The h2 connection driver is a `tokio::spawn` onto whichever runtime is current (`crates/rustkit-http/src/lib.rs:525`).

### 1.2 Everything RustKit does, it does on the window thread

- **Ownership.** `RustKitView { engine: RefCell<Engine>, live_runtime: Option<tokio::runtime::Runtime>, … }` is in `crates/hiwave-app/src/webview_rustkit.rs:36-66`. Its doc comment says it is not Send/Sync and main-thread only (`:33-35`). It is wrapped in `Arc` twice (`main.rs:1217`, `main.rs:1972`), but only the loop closure touches it (`main.rs:1979`).
- **One content view for every tab.**
  - Tab switch: `ActivateTab` sends `UserEvent::Navigate(tab url)` (`main.rs:797-819`).
  - Tab close: `CloseTab` sends `Navigate(next active tab)` (`main.rs:783-796`).
  - Workspace switch also navigates (around `main.rs:891`).
  - So switching to a tab re-loads it in full, and that load blocks the window thread.

### 1.3 Where the window thread blocks

| call (window thread) | site | bounded by |
|---|---|---|
| **Load** `load_url_blocking`: builds a fresh current-thread runtime and runs `rt.block_on(engine.load_url(..))` | `webview_rustkit.rs:515-547` (block at `:538-542`) | per-fetch timeouts; the script budget **15 s** (`LIVE_SCRIPT_BUDGET_MS`, `webview_rustkit.rs:78`); interrupt at host calls (`:95`). A load is network, parse, subresources, scripts, load-time timers and layout. The comment at `main.rs:1615-1631` measured it at "minutes, on a heavy page in a debug build". |
| Callers of the load | `Navigate` (`main.rs:2536` → `content_webview_enum.rs:30` → `webview_rustkit.rs:464-488`); back, forward and reload (`main.rs:2563`, `:2587`, `:2611` → `webview_rustkit.rs:238-278`) | as above |
| `load_html_internal` | `webview_rustkit.rs:425-434`; used by new tab, about and report pages (`main.rs:504-515`, `:2470`, `:2640`, `:4236`) | parse + scripts, no network |
| **Live turn** `process_events`: `runtime.block_on(engine.pump_live(..))` | `webview_rustkit.rs:180-204`; called at `main.rs:2327` and `:2450` | **no wall-clock bound on script.** Up to `MAX_LIVE_TIMER_CALLBACKS` = 1000 callbacks (`crates/rustkit-engine/src/lib.rs:767`); network polled ≤ 2 ms (`LIVE_NETWORK_SLICE`, `lib.rs:781`); dynamic `import()` up to `LIVE_NETWORK_BUDGET` = 2 s (`lib.rs:773`); Boa loop limit 10M iterations per frame (field `lib.rs:711`, value `lib.rs:750`) |
| **Input dispatch**: `click_at_point`, `mouse_down/move/leave`, `handle_text_key`, `handle_key_up`, `submit_focused_form`, `focus_at_point`, `scroll_view` | `main.rs:2335-2447` → `webview_rustkit.rs:285-422` → engine `lib.rs:1924-3146` | **no wall-clock bound.** Each one runs page listeners synchronously (`fire_mouse` `lib.rs:2842`, `fire_key` `lib.rs:3184`) and then lays out |
| `relayout` after an edit | `main.rs:2201`, `:2238`, `:2429` → `lib.rs:4591` | full layout of the page |
| **Render** | `webview_rustkit.rs:215-229` → `render_changed_views` (`lib.rs:12711`) → `render` (`lib.rs:13466`): `get_surface_texture`, `renderer.execute(commands)`, `compositor.present` | display-list size. Wikipedia's portal is 2.2M commands (`webview_rustkit.rs:206-214`); only changed frames are drawn since 2026-10-08 |
| `execute_script` (inspector, zoom, console, `window.stop()`) | `webview_rustkit.rs:550-563`; `main.rs:2630`, `:4232-4567` | no deadline |
| Shield check per subresource (inside the load) | `shield_adapter.rs:354` `recv_timeout(250ms)` | 250 ms each |
| Modal file dialogs (`rfd`) | around `main.rs:3544-3590` | the user |
| wry IPC handlers: `ipc::commands::handle_message` under `Arc<Mutex<AppState>>` | `main.rs:739-940`, `ipc/commands.rs:30` | short; they run inside WKWebView's callback on the main thread |

### 1.4 The script budget and the stop path

- **Where the deadline is armed.** `JsRuntime::set_execution_deadline` (`crates/rustkit-js/src/lib.rs:343-354`) is armed at exactly one place: `run_page_scripts` (`crates/rustkit-engine/src/lib.rs:3678-3682`), on the load path. Those are its only non-test call sites.
  - So a timer callback in `pump_live`, an event listener, a fetch callback or `execute_script` after the load has **no deadline**.
- **What the deadline does.** It is checked only at the top of each host function (`rustkit-js/src/lib.rs:481-504`). Past the deadline, that check returns `RuntimeLimitError::LoopIteration`, which script cannot catch.
  - Script that never calls the host is not stopped (`rustkit-js/src/lib.rs:346-349`). PR #616 measured 8.0 s of host-free script under a 3.5 s budget.
- **Stop button.** `UserEvent::Stop` evaluates `window.stop();` in the page, with `// TODO: Implement stop in RustKit` (`main.rs:2625-2636`). `window.stop` is not defined in rustkit-bindings.
  - `Engine::stop` exists (`lib.rs:9657`). It bumps `nav_generation`, and the load checks that generation after its awaits (`lib.rs:4144` … `4420`). The socket is not aborted (`lib.rs:9645-9656`).
  - The app never calls `Engine::stop`. It could not run during a load anyway: the load holds `&mut Engine` inside `block_on` on the same thread.
- **How long the window freezes today:**
  - **A runaway in a load-time script**: up to 15 s, then it stops at the next host call. With no host calls, until the loop limit, or until it ends.
  - **A runaway in a timer or handler after load**: unbounded except by the 10M loop-iteration limit. A `forEach`/`reduce`/recursion spin is not a loop statement, so nothing stops it.
  - **A slow network load**: as long as the load takes.

### 1.5 Input path (issue #575)

- **Queues.** `RustKitContentView`'s AppKit selectors (`crates/rustkit-viewhost/src/macos.rs:139-330`) push into three process-global statics: `PENDING_CLICKS` (`:64`), `PENDING_KEYS` (`:89`) and `PENDING_SCROLLS` (`:116`).
- **No view identity.** None of the entries carries a view id. `PendingScroll` is `{x, y, dx, dy}` (`:105-110`).
- **Drained by type, not in arrival order.** `MainEventsCleared` runs these steps in this order (`main.rs:2320-2463`):
  1. `process_events`
  2. all clicks
  3. all scrolls (`:2383`)
  4. all keys (`:2391`)
  5. `process_events`
  6. `take_script_navigation`
  7. `render`
- **Wake-up.** Nothing wakes the loop explicitly. The NSEvent itself wakes the tao run loop.
- **What survives.** The `KeyboardInput` arm at `main.rs:2146-2261` is still live. The `MouseInput` arm (`main.rs:2087-2145`) is dead: child NSView events never reach tao (`main.rs:2331-2334`).

### 1.6 GPU surface

- **Device.** `Compositor::with_config` (`crates/rustkit-compositor/src/lib.rs:130-195`) creates the wgpu Instance, Adapter, Device and Queue on whichever thread builds the `Engine` (`lib.rs:1438`). In the app that is the main thread. Every `Engine::new` makes its own device, including the unused `RustKitChromeView` (`crates/hiwave-app/src/webview_rustkit_chrome.rs:65`, `:115`).
- **Surface.** It is created in `Engine::create_view` (`lib.rs:1612-1693`) from the NSView raw handle (`rustkit-compositor/src/lib.rs:284-310`).
- **Present.** It is synchronous on the caller (`lib.rs:13619`, compositor `:617`).

---

## 2. Constraints found in the code

### 2.1 Types that cannot cross threads (must be confined)

| type | why it is !Send | site |
|---|---|---|
| `rustkit_dom::Document`, `Node` | `Rc<Node>`, `RefCell<Option<Weak<Node>>>`, `Cell` | `crates/rustkit-dom/src/lib.rs:93-106`, `:270-280` |
| DOM event listeners | `Box<dyn Fn(&DomEvent) + 'static>` (no Send) | `rustkit-dom/src/events.rs:354` |
| `DomBindings` | `RefCell<JsRuntime>`, `Rc<Cell<DomDirty>>`; `SharedDomHost = Rc<RefCell<DomHost>>` | `crates/rustkit-bindings/src/lib.rs:533-546`, `dom.rs:100`, `web_scroll.rs:22` |
| `JsRuntime` | `boa_engine::Context`, `Rc<HostJobExecutor>`, `Rc<ExecutionDeadline>`; host functions are `Box<dyn Fn>` capturing `Rc`s | `rustkit-js/src/lib.rs:106`, `:151-165` |
| `HostJobExecutor` | `RefCell<&'static mut Context>`, created with `transmute`; raw pointers | `rustkit-js/src/executor.rs:52-63`, `:177` |
| Boa `Context` / `Gc` | the GC heap is `thread_local!` (`boa_gc-0.22.0/src/lib.rs:45`); `Gc<T>` carries `PhantomData<Rc<T>>` (`pointers/gc.rs:157`). Boa's own doc: contexts "have to be in the same thread" (`boa_engine-0.22.0/src/context/mod.rs:52-55`). | Boa 0.22.0 (`crates/rustkit-js/Cargo.toml:18`) |
| `ViewState` | all of the above, plus `LiveFuture = Pin<Box<dyn Future>>` with no `+ Send` | `rustkit-engine/src/lib.rs:509-642`, `script_net.rs:121` |
| `Engine` | `HashMap<_, ViewState>`, `RefCell` style trace, `Cell`s; layout borrows `*const Document` (`lib.rs:5191`) | `lib.rs:1251-1307` |
| `RustKitView` | `RefCell<Engine>`, `Cell`s | `webview_rustkit.rs:36-66` |

**Consequences:**
- **Construct on the owning thread.** A page's `Engine` (or whatever part holds DOM and JS) must be **constructed on the thread that will own it**, and it never moves. This already works: tests build an `Engine` on a helper thread (`on_helper_engine`, `rustkit-engine/src/lib.rs:128-149`).
- **The executor must stay single-threaded.** Its `'static` transmute is only sound because futures are polled only inside `run_jobs` on one thread (`executor.rs:163-176`).
- **No cross-thread workarounds.** No `unsafe impl Send` exists anywhere in `crates/`, and the design must not add one for these types.

### 2.2 Types that can cross (checked)

A scratch crate compiled `fn s<T: Send + Sync>()` against these, on Linux, with the lockfile from `f54103d`. All passed:

- `rustkit_layout::DisplayList` (`crates/rustkit-layout/src/lib.rs:7907`, `#[derive(Clone)]`, glyph runs are `Arc<GlyphRun>`)
- `rustkit_net::ResourceLoader`
- `rustkit_image::ImageManager`
- `rustkit_layout::text::FontLoader`
- `rustkit_net::RequestInterceptor`

**Unverified on macOS.** These were compiled on Linux only. The macOS cfgs, for example CoreText handles in the font path, may differ. Confirm with a `static_assertions`-style test in `rustkit-engine` built on the Mac (migration step M3 adds it).

**Plain data.** `PendingClick`, `PendingKey` and `PendingScroll` are plain data. `EngineViewId` is a value. tao's `EventLoopProxy` is Send.

### 2.3 Boa's threading and interruption model

- **One thread per context.** A Context and every object it allocates live on one thread: the GC heap is thread-local. Several Contexts on different threads are fine, each with its own heap.
- **No preemption or yield.** Boa has no way to suspend a running script and resume it later, except through generators and async functions, which the page writes, not us.
  - So "cooperative/preemptible" can only mean **terminate**, never **pause**.
- **Runtime limits in 0.22** are recursion 512, stack 10 240 slots, and loop iterations `u64::MAX` by default (`boa_engine-0.22.0/src/vm/runtime_limits.rs:19-26`). They are checked in two places:
  - **`Context::check_runtime_limits`** (`boa_engine-0.22.0/src/vm/mod.rs:1017-1034`), called on **every function call**: ordinary calls at `builtins/function/mod.rs:991` and `:1108`, native calls at `native_function/mod.rs:337` and `:392`.
  - **`IncrementLoopIteration`** (`vm/opcode/iteration/loop_ops.rs:11-22`), on every loop back-edge, with a per-frame counter.
- **What a patch would cover.** Adding an interrupt check to those two places covers every loop statement and every call. That includes the callbacks `forEach`/`reduce` make, which is the ebay case in PR #616.
  - **What it would miss.** A single long native operation such as a pathological regex, or `Array(1e8).fill()`. Those end on their own.
  - **The error.** `RuntimeLimitError` is uncatchable by script, the property #616 already relies on.
- **A patch is required.** Upstream 0.22 has no interrupt hook (grep of `boa_engine-0.22.0/src` for an interrupt API: none). That means carrying a patched `boa_engine`. The repo has done this before: `[patch.crates-io]` for `third_party/boa_gc` and `boa_parser`, now stale (`Cargo.toml:138-142`, listed as `[[patch.unused]]` in `Cargo.lock`).

### 2.4 wgpu / Metal surface ownership

- **Surface creation is main-thread only.** wgpu-hal 24 **panics** if a surface is created from an NSView off the main thread (`wgpu-hal-24.0.4/src/metal/surface.rs:65-67`, "get_metal_layer cannot be called in non-ui thread").
- **Capabilities lose the extent off-main.** `surface_capabilities` off the main thread returns no current extent and logs a warning (`wgpu-hal-24.0.4/src/metal/adapter.rs:341-345`). Configuring and presenting off-main therefore works only with an explicit size passed in.
- **Device and queue are thread-safe.** They are `Arc<Device>` and `Arc<Queue>`, Send + Sync in wgpu 24, so encoding on another thread is allowed.
- **Rule for this design.** NSView and CAMetalLayer creation, resize and present stay on the window thread for now. Moving encode+present to a render thread is a later, separate step (M9) that needs the explicit-extent configure.

### 2.5 AppKit main-thread rules

These must run on the main thread:
- NSWindow and NSView creation
- `addSubview:`, `setFrame:`, `makeFirstResponder:`
- tracking areas and cursor changes
- `NSMenu`, which muda drives
- modal panels (`rfd`)
- `NSApplication` run and terminate

The code has **no main-thread assertions at all**: no `MainThreadMarker`, `isMainThread` or `pthread_main_np` in `crates/`. Every `msg_send!` relies on convention. The view host should add `MainThreadMarker`-style checks as it takes over (M8).

### 2.6 What tao and wry do for us that the view host must take over (macOS)

| today provided by | function | where | view-host replacement needed |
|---|---|---|---|
| tao | `NSApplication` setup, activation, the run loop | `main.rs:651`, `:2040` | `ViewHost::run(handler)` |
| tao | **cross-thread wake** (`EventLoopProxy::send_event`) | `main.rs:652`; all IPC handlers | a Send `WakeHandle` (CFRunLoopSource or `dispatch_async(main)`). **The page-thread design depends on it.** |
| tao | `ControlFlow::WaitUntil` timers | `main.rs:2041-2044` | a run-loop timer |
| tao | window create, icon, close, resize, scale-factor change, focus | `main.rs:655-663`, `:2281`, `:2298-2319`, `:682` | NSWindow delegate |
| tao | key events and modifiers for the window, plus IME on its own view | `main.rs:2074-2261` | `RustKitContentView` already takes keys (`macos.rs:262-300`), with **no IME**: it needs `NSTextInputClient` |
| tao | second window (settings) | `main.rs:3175-3200` | a multi-window view host |
| muda (works on NSApp, not on tao) | menu and accelerators; required for clipboard shortcuts | `crates/hiwave-app/src/platform/macos.rs:12-151`, `main.rs:671-676` | can stay |
| wry | chrome, shelf, inspector and settings UI as HTML in WKWebView | `main.rs:733`, `:1661`, `:1759` | RustKit chrome document (Phase X step 2) |
| wry | JS↔Rust IPC (`window.ipc.postMessage`) | `main.rs:739` … | chrome page ↔ window thread messages (section 4) |
| wry | clipboard in the chrome (`with_clipboard(true)`), drag-and-drop, devtools | `main.rs:733` | NSPasteboard and drag-in on the view host |

**Windows and Linux in this repository.** On non-macOS the content is wry as well (`#[cfg(any(not(target_os = "macos"), feature = "webview-fallback"))]`, `main.rs:1220`). `rustkit-viewhost` has a Windows implementation (`crates/rustkit-viewhost/src/lib.rs:35-222`) and nothing for Linux.

---

## 3. Options

The same questions are answered for each option:
1. What moves.
2. What must be Send or confined.
3. How a frame reaches the screen.
4. How input reaches the page.
5. How a runaway is stopped.
6. Cost.
7. Risk.

**Terminating a runaway, common to all options.** A thread does not stop a loop. Rust cannot kill a thread, and Boa cannot be paused. Every option ends a runaway with the same two mechanisms:

- **T1, host-call refusal.** This is #616's check (`rustkit-js/src/lib.rs:481-504`). It keeps its local wall-clock deadline (`Rc<ExecutionDeadline>`, `:140-148`) and additionally reads the shared **interrupt word** defined below. It is armed on every live turn, input dispatch and `execute_script`, not only on the load.
- **T2, VM interrupt (needs patched Boa).** `check_runtime_limits` and `IncrementLoopIteration` read the same interrupt word and return `RuntimeLimitError` when it targets the running turn. This is about 30 lines in a vendored `third_party/boa_engine`. The cost is one relaxed atomic load per call and per loop back-edge.

These two stop everything except a single long native call. What the options differ in is **who is frozen until T1/T2 fire, and who sets the word**.

**Interrupt word: lifecycle (shared by every option).** Two control blocks, Send + Sync, atomics only. Turns are per **thread** (one thread runs one turn at a time); stop state is per **view** (each view has its own Boa context, and from M4 to M5 one thread hosts several views):

```rust
struct ThreadStamp {              // one per page thread; written only by that thread
    turn: AtomicU64,              // id of the turn running now; 0 = idle. Ids only grow, never reused.
    turn_started_ms: AtomicU64,   // start of `turn`
    turn_view: AtomicU64,         // which view's script `turn` runs
}
struct ViewControl {              // one per view
    interrupt: AtomicU64,         // a turn id to stop, or ALL (= u64::MAX); 0 = none
    cancel_nav: AtomicU64,        // cancel every load whose nav id is <= this; 0 = none
}
```

- **Turn ids, not a flag.** The page thread starts every unit of script work for a view (a load, a live turn, one input dispatch, one eval) by taking the next turn id, publishing it (order below), and telling that view's `JsRuntime` the id (`set_turn(id)`; for T2 this writes the id into Boa's `RuntimeLimits`, which the page thread owns). T1 and T2 stop the script when the view's `interrupt == current turn` or `interrupt == ALL`.
- **Writers never lower the word.** Every writer of a turn id uses `interrupt.fetch_max(id)`. Ids only grow and `ALL` is `u64::MAX`, so:
  - a turn id never replaces `ALL` (a watchdog or Stop that fires after "Stop scripts on this page" leaves `ALL` in place);
  - an older id never replaces a newer one;
  - a stale id is harmless: once turn N has ended, the next turn is N+1 and does not match, so **a turn-targeted value is never cleared**.

  `ALL` is set with `store(ALL)` (it is the maximum, so it equals `fetch_max(ALL)`).
- **`ALL` is cleared by one writer, one way.** `ALL` means "this view runs no more script". It is set by the user's "Stop scripts on this page" (the busy bar, open question 2) and by closing a view (M5 on). The page thread clears it only when a new document commits in that view (`load_url_with_disposition` installs new bindings), with `compare_exchange(ALL, 0)`, so a navigation, and only a navigation, re-enables script. A turn id stored after that clear targets a turn that already ended.
- **Publishing a turn without tearing.** The page thread, at turn start: `turn_view.store(v, Relaxed)`, `turn_started_ms.store(now, Relaxed)`, then `turn.store(id, Release)`. At turn end: `turn.store(0, Release)`. The watchdog reads `t1 = turn.load(Acquire)`, then `turn_started_ms` and `turn_view`, then `t2 = turn.load(Acquire)`, and acts only if `t1 == t2 != 0`. Because every turn ends by publishing 0 before the next turn writes its start time, an unchanged nonzero `turn` across the two reads means the start time and view belong to that turn: the watchdog cannot judge a fresh turn by the previous turn's clock. If `turn` changed, it skips this wake and reads again on the next.
- **Merge with `ExecutionDeadline`.** `ExecutionDeadline` stays the page thread's own clock (the load's 15 s budget, set and lifted by the page thread exactly as at `rustkit-engine/src/lib.rs:3678-3682`). It gains a field `control: Option<Arc<ViewControl>>` plus a `Cell<u64>` with the current turn, and the host-function check becomes `deadline passed || word matches`. `hit`/`late` reporting (`take_deadline_hit`, `log_script_stopped`) is reused for both, with the reason recorded (`OverBudget`, `Watchdog`, `UserStop`).

**Load cancellation (needed for Stop during a load).** The interrupt word ends **one script**; a load is many scripts, subresources and layout inside one `load_url` future that holds `&mut Engine`, and `PageCmd::Stop` would wait in the channel behind it. Dropping that future is not safe: `run_page_scripts` takes `view.bindings` out (`lib.rs:3670`) and puts it back only after the await (`lib.rs:3683-3686`). So the load is cancelled **cooperatively, from inside**, and is never dropped.

**Nav ids come from the window thread.** Every load of a view starts from a window-thread command: the URL bar, back and forward, reload, tab activation, and the page's own navigations (which arrive as `PageEvent::NavigateRequested` and go back as a command, section 3A input path). So the window thread numbers them: `PageCmd::Navigate { nav_id, .. }`, with `nav_id` increasing per view. The page thread records the `nav_id` on the load and in `ViewState` next to `nav_generation`. The window thread therefore always knows the newest load it has asked for, including one the page thread has not started yet; it never depends on a `LoadState` event having arrived.

1. Stop stores `cancel_nav.fetch_max(last_sent_nav_id)` for the view and `interrupt.fetch_max(turn)` when `turn_view` is that view.
2. `nav_superseded` (`lib.rs:9689`), already checked after every await of the load (`lib.rs:4144` … `4420`), also returns true when `nav_id <= cancel_nav`.
3. The same check is added **between scripts** in `run_page_scripts_with`, at the point where the budget is already read (`lib.rs:3821`), and between subresource phases. A cancelled load marks the remaining scripts as not run, the way `OverBudget` does today.
4. In-flight fetches race a `tokio::sync::Notify` (Send) in the `timeout_at` already wrapped around them (`lib.rs:3582-3635`, `10291`, `10457`, `10505`, `10818`). The socket is dropped with the future; the loader needs no new cancellation API for that.
5. The load then **returns** through its existing superseded path, so `run_page_scripts` restores `view.bindings` normally. The page thread then handles the queued `PageCmd::Stop` and runs `Engine::stop` (`lib.rs:9657`) for the `NavigationFailed` event and state.
6. **A queued `Navigate` whose `nav_id <= cancel_nav` is dropped** when the page thread takes it off the channel, so a Stop sent between loads, or before the page thread has started the newest one, cancels it too. A later Navigate (the user's next one) has a larger id and is not affected: Stop never needs clearing.

This lands as its own step (M3b, section 6), testable in the engine before any thread exists, because the cancel is set from another thread through an `Arc`.

### Option A: a page thread per tab; the window thread owns input, chrome and presentation

```
                 window thread (main / AppKit)                         page thread (tab 1)          page thread (tab 2)
 ┌──────────────────────────────────────────────────────────┐   ┌───────────────────────┐   ┌────────────────────┐
 │ run loop (tao now, viewhost later)                        │   │ PageEngine: DOM, Boa, │   │ PageEngine ...     │
 │ NSWindow, NSViews (one per tab view + chrome view)        │   │ style, layout,        │   │                    │
 │ ordered input queue (seq, view id) ──route by view──────────►│ net (own tokio rt)    │   │                    │
 │ Presenter: Compositor + Renderer + surfaces               │   │ timers, live turns    │   │                    │
 │   latest FrameCommit per view ◄──────────────commit──────────┤ commit(DisplayList)   │   │                    │
 │   draws front view's latest commit, presents              │   │                       │   │                    │
 │ watchdog: turn_started / interrupt word per page ───set─────►│ Arc<AtomicU64>        │   │                    │
 │ tab model, stop/close/switch (native, no JS)              │   └───────────────────────┘   └────────────────────┘
 └──────────────────────────────────────────────────────────┘
```

**What moves.** `Engine` is split in two:

- **`PageEngine`, per page thread.** It holds `views` (`ViewState`: DOM, bindings, layout, display list), `loader`, `image_manager`, `font_loader`, `svg_cache` and the style trace.
- **`Presenter`, on the window thread.** It holds `viewhost`, `compositor` and `renderer`.

`RustKitView` stops holding `RefCell<Engine>`. It holds a `PageHandle { tx: Sender<PageCmd>, stamp: Arc<ThreadStamp>, control: Arc<ViewControl>, view: ViewId, last_sent_nav_id: u64 }`.

**Send vs confined.**

- **Confined to the page thread** (built there and never moved): everything in 2.1.
- **Crosses the boundary:**
  - **`PageCmd`**: Navigate(url), LoadHtml, Stop, Reload, Input(seq, InputEvent), Resize(bounds, scale), Eval{script, reply: oneshot}, SetVisible, Close.
  - **`PageEvent`**: Commit(FrameCommit), Title, Url, LoadState, NavigateRequested(url, disposition), Focused(bool), Cursor, Unresponsive, Closed.
  - **`FrameCommit { view, generation, list: Arc<DisplayList>, content_size, scroll, image_refs }`**: Send by 2.2.
- **Shared by `Arc` across page threads** (Send + Sync per 2.2): `ResourceLoader` (cookie jar and HTTP cache) and `FontLoader`. `ImageManager` can be shared or kept per page.

**How a frame gets to the screen.**

1. At the end of a live turn, or after a load step, the page thread calls today's `flush_script_dom_writes` / relayout.
2. If `frame_generation` moved, it sends `Commit`, then wakes the window thread through the proxy (Send).
3. The window thread stores the latest commit per view and drops older ones.
4. On `MainEventsCleared` it renders the **front** view's latest commit with today's code path (`lib.rs:13466-13625`): get texture, execute, present.

Scroll is applied by the presenter as the `PushTransform` it already wraps (`lib.rs:13589-13600`). That makes window-thread scrolling of a busy page's last frame possible, but it gives the offset two writers (the presenter, and the page's `scrollTo`/relayout). The rule:

- **The page thread is the only owner of the scroll offset.** Every commit carries the offset it was laid out at and `scroll_ack`, the `seq` of the last wheel input it consumed.
- **The presenter only predicts.** It keeps the wheel deltas it has forwarded with `seq > scroll_ack` and draws `committed offset + unacknowledged deltas`, clamped to the committed content size. When a commit arrives, acknowledged deltas are dropped.
- **So the page wins.** A `scrollTo` in the page shows up in the committed offset, and only the user's input that the page has not seen yet is added on top, as in Chrome's compositor scrolling.

Until the step that adds this (M9b, section 6), wheel input goes to the page thread like any other input and the presenter draws the committed offset only.

**How input reaches the page (fixes #575 by design).**

1. **One queue in the view host.** `InputEvent { seq: u64, at: Instant, view: ViewId, kind }` lives in a single `Mutex<VecDeque<…>>` in `rustkit-viewhost`. Each `RustKitContentView` knows its `ViewId`, from an ivar set at `create_view`.
2. **Drained in `seq` order** on the window thread, then routed by `view`:
   - Events for a view that no longer exists are dropped.
   - Wheel and move events for a view that is not front are dropped.
3. **Coalesced per view, adjacent only.** A run of moves or wheels is merged only with its immediate neighbours, never across a click or key. That keeps today's summing (`macos.rs:221-252`) without reordering.
4. **Sent as `PageCmd::Input(seq, ev)`.** The page thread also checks a **document generation** (`nav_generation`, `lib.rs:550`) stamped by the window thread when it routed the event, and drops input aimed at a document that has since been replaced.
5. **Synchronous answers become replies.** Today the window thread asks several questions synchronously: `handle_text_key → bool`, `form_submit_url`, `link_at_point`, `has_focused_element`, `take_script_navigation`.
   - The page thread now decides them itself: a key not consumed scrolls inside the page thread, and a submit or link becomes `PageEvent::NavigateRequested`.
   - Focus changes become `PageEvent::Focused`.

**Runaway after load, then switch, stop and close.**

- **Watchdog.** The window thread does not run page code, so it stays responsive. Each wake, it reads each page thread's `ThreadStamp` (the torn-read-safe order above) and compares the turn's age against the budget: 15 s during a load and `LIVE_TURN_BUDGET` after (open question 2). Past the budget it does `interrupt.fetch_max(turn)` on `turn_view`'s `ViewControl`, and T1/T2 unwind that script only.
- **Stop during a load.** The window thread does `cancel_nav.fetch_max(last_sent_nav_id)` and, if this view's script is running, `interrupt.fetch_max(turn)`, then sends `Stop`. The running script unwinds (T2, or T1 at its next host call), the load sees `cancel_nav` at its next check and returns through its superseded path, any not-yet-started load for the view is dropped from the queue, and then `Stop` is handled (load cancellation, above).
- **Stop after load.** There is no load to cancel. The window thread does `interrupt.fetch_max(turn)` for this view, which ends the running script. If the page keeps starting new runaway turns (a `setInterval` spin), the busy bar offers "Stop scripts on this page", which stores `ALL` until the next navigation.
- **Close tab.** The window thread removes the tab from the strip at once, then, depending on the step:
  - **M4 (one shared view):** the closed tab's page and the next tab's page are the same view, so the view must keep running script. The window thread does `cancel_nav.fetch_max(last_sent_nav_id)` and `interrupt.fetch_max(turn)` (never `ALL`), **then** sends `Navigate { nav_id: last_sent_nav_id + 1 }` for the next tab. The store happens before the send on the same thread, and the new id is larger than `cancel_nav`, so the next tab's load is neither cancelled nor interrupted. The `Sender` is kept.
  - **M5 (retained views, one thread):** the view is removed from the presenter and its NSView from the window. The window thread stores `ALL` on that view's `ViewControl`, does `cancel_nav.fetch_max(last_sent_nav_id)`, then sends `CloseView(view)`; the page thread destroys the view's state when the command is reached. Other views on the thread are unaffected, because `ALL` is per view. The `Sender` is kept.
  - **M6 (a thread per tab):** as M5, then the `Sender` is dropped. The page thread exits when its loop sees the closed channel; if it does not acknowledge within 2 s it is abandoned (below).
  - **Abandonment (M6).** If a page thread does not acknowledge within 2 s (a single long native call), it is **abandoned**: detached, logged, and its CPU use reported. Rust cannot kill it; only a process boundary can (open question 4).
- **Switch tab.** This is a presentation change on the window thread. It never waits on any page.

**Cost.**

- **Memory.** One OS thread per live tab. Boa needs deep native stacks: `docs/BOA_UPGRADE_PARKED_2026-10-03.md:30-32` records the 16 MB Windows main-thread stack against the recursion limit. So each page thread is spawned with an explicit 16–32 MB stack, which is virtual reservation, not committed memory.
- **Retained tabs.** Keeping each tab's DOM alive instead of reloading on switch is the real memory cost: tens to hundreds of MB per heavy page. It is bounded by discarding background tabs (Tab Decay) — open question 3.
- **GPU.** One wgpu device in total, instead of one per `Engine` as today.
- **Latency.** Input costs one channel hop plus one wake (sub-millisecond). A frame is drawn on the next window-thread turn after the commit, at most one turn later than today.

**Risk.**

- The split of `Engine` touches a 36k-line file. The places where engine code assumes it owns the compositor (`render`, `create_view`, `set_view_bounds`, image upload) must be found by compiling.
- `ImageManager` → GPU texture upload must move to the presenter side. Find how textures are created today (`grep -n "image" crates/rustkit-renderer/src/lib.rs`) before M3.
- Ordering bugs between commits and resize. Mitigated: commits carry the size they were laid out for, and the presenter scales or letterboxes a stale one.

### Option B: one content thread for all tabs

```
 window thread                                content thread (one)
 ┌─────────────────────────────┐   cmds     ┌────────────────────────────────────┐
 │ input queue, routing        │──────────► │ PageEngine { views: tab1, tab2, … } │
 │ Presenter, surfaces         │◄────────── │ one Boa heap per view (same thread)│
 │ watchdog, interrupt words   │  commits   │ one tokio current-thread runtime   │
 └─────────────────────────────┘            └────────────────────────────────────┘
```

- **What moves:** the same split as A, but one `PageEngine` with all views, which is the shape `Engine` already has (`views: HashMap`).
- **Send vs confined:** identical to A.
- **Frames and input:** identical to A. The view id travels with every message, and that matters more here because one thread serves several views.
- **Runaway:** the window stays responsive. Stop and close of the runaway tab work through its interrupt word.
  - **Every other tab's page is frozen behind it** until T1/T2 fire. Their last frames still show and scroll (presenter scroll), but their timers, clicks and loads wait.
  - Switching to another tab shows its last frame at once. It stays inert until the runaway is interrupted.
- **Cost:** one extra thread and one runtime. Least memory of the threaded options.
- **Risk:** lowest of the threaded options, because it is the same object graph on another thread. But it is a dead end for the user goal: "the user can switch tabs" is only half met, and one slow page (not a runaway, just a 5 s layout) stalls every tab.

### Option C: keep one thread; make script cooperative with a hard budget

```
 window thread
 ┌───────────────────────────────────────────────────────────┐
 │ loop turn: input → page turn (deadline D) → render         │
 │   Boa checks interrupt in calls/loops (T2) against now()>D │
 │   load: async steps resumed across turns (no block_on)     │
 └───────────────────────────────────────────────────────────┘
```

**What moves:** nothing. Two things change:

- Every script entry gets a short wall-clock deadline: T2 against `Instant`, plus T1.
- `load_url` stops being `block_on`. The load future is stored and polled a slice per turn, the way `pump_live` already polls `LiveFuture`s (`script_net.rs:143-167`).

**Send:** nothing needs to be Send.

**Frames and input:** same thread, same as today. #575 is fixed separately (M2).

**Runaway:** terminated at the deadline. A terminated script is gone: Boa cannot resume it.

- **Per-turn deadline.** To keep the window under about 100 ms per turn, the per-turn deadline would have to be about 100 ms. Legitimate long scripts would then be killed, for example YouTube's base module, which ran past 5 s (`webview_rustkit.rs:68-71`).
- **Seconds-long deadline.** With a deadline in seconds, the window freezes for seconds.
- **Uncovered work.** Layout and parse are not covered by the deadline at all. Wikipedia's 2.2M-command list is laid out and drawn on this thread.

**Cost:** the cheapest option.

**Risk:**
- Low technical risk.
- Does not meet the goal: browser controls are not responsive while page code is legitimately busy.
- Cannot coexist with a RustKit-drawn chrome without the chrome freezing whenever the page does.

### Option D (recommended): Option A's architecture, delivered through B, with C's terminators

D is A's end state, with these changes:

1. **One `PageThread` type** (spawn, construct the `PageEngine` on the thread, a message loop with its own tokio runtime, an interrupt word, a watchdog stamp). The first landing **instantiates it once for all content** (B's shape). Later landings instantiate it **per tab** (A). The protocol does not change between the two.
2. **Terminators T1 and T2 land first, on today's single thread** (C's useful half). They bound the freeze before any thread moves, and they are what later stops runaways on page threads.
3. **The chrome document is a `PageThread` too**, with a privileged message set. The window thread runs **no JS and no layout at all**. Stop, close tab, next and previous tab, and quit are **native accelerators** handled on the window thread (muda menu items, `platform/macos.rs`). They work even if the chrome document's thread is busy.
4. **Background tabs.** Background tabs beyond a cap are discarded to (URL, last frame), in line with Tab Decay. A discarded tab's thread exits, and it reloads on activation.

---

## 4. Interaction with Phase X

### 4.1 Chrome UI drawn by RustKit

- **Which thread.** The chrome (tabs, URL bar, shelf, find-in-page) is **a separate RustKit document in its own view, on its own page thread**. It is not on the window thread.
  - **Why not the window thread.** The chrome's scripts and layout would put JS and layout back on the window thread. A bug in our own chrome script, or a slow layout of a 300-tab strip, would then freeze page presentation and input routing. It would also need a second Boa heap there.
  - **Cost of the separate thread.** A click on a tab goes view host → window thread → chrome thread → `ChromeEvent::ActivateTab` → window thread. That is two channel hops, well under a frame.
- **Messages.** The chrome document talks to the window thread by message, replacing `window.ipc.postMessage` (`main.rs:739`). Two binding families provide this:
  - `hiwave.send(cmd)` → `ChromeEvent` (same shapes as `IpcMessage`, `crates/hiwave-app/src/ipc/mod.rs`).
  - `ChromeCmd::Update(json)` → `evaluate`, replacing `evaluate_script("hiwaveChrome.updateUrl…")`.
- **`AppState`.** It stays in `Arc<Mutex<AppState>>` on the window thread, as today. It is never touched from a page thread.
- **What must not route through the chrome document.** Stop, close tab and switch tab go through native accelerators and menu items on the window thread. The chrome's buttons are a second path to the same window-thread actions. So "stop works while a page spins" never depends on any JS thread.
- **Env flag.** Behind `HIWAVE_RUSTKIT_CHROME=1`, the chrome page thread replaces the wry chrome webview. The window thread code is the same either way: it receives `ChromeEvent`s from wry IPC or from the chrome page thread through one enum.

### 4.2 `rustkit-viewhost` owns the NSWindow (macOS)

**Starting point.** After M4 (section 6) the window thread's job is only:
- run the loop
- own the windows and views
- drain one ordered input queue
- route `PageCmd` and `PageEvent`
- present commits
- run the watchdog
- handle native menus and accelerators

**What replacing tao takes.** The work behind the run loop is swapping its provider. The view host must supply:
- `run(handler)`
- a Send `WakeHandle`, the replacement for `EventLoopProxy`
- `WaitUntil` timers
- window lifecycle events (resize, scale, close, focus)
- the ordered input queue (M2), now covering the chrome view too
- IME (`NSTextInputClient`) on `RustKitContentView`
- multiple windows (settings)

**Main-thread checks.** All AppKit calls stay on the window thread. Add `MainThreadMarker`-style checks in the view host as part of this step (2.5).

### 4.3 Windows and Linux on tao in the meantime

- **The page-thread protocol is platform-neutral.** `PageThread`, `PageCmd`/`PageEvent` and `FrameCommit` use no AppKit and no tao.
- **What differs per platform is the window thread's host.** It goes behind one small trait in `hiwave-app`:
  ```rust
  trait WindowHost {
      fn wake_handle(&self) -> WakeHandle;            // Send + Clone
      fn create_content_view(&self, parent, bounds) -> ViewId;
      fn drain_input(&self) -> Vec<InputEvent>;       // ordered, view-tagged
      fn raw_handle(&self, ViewId) -> RawWindowHandle; // for Presenter surfaces
  }
  ```
- **Implementations:**
  - `TaoHost` on all three platforms first. Its wake is `EventLoopProxy`, and its input comes from tao `WindowEvent`s tagged with the content view id.
  - `ViewHostMac` replaces it on macOS at Phase X step 3.
  - Windows can move to `rustkit-viewhost`'s Windows backend later (`rustkit-viewhost/src/lib.rs:35-222`). Linux stays on `TaoHost`.
- **Content on Windows and Linux.** In this repository, Windows and Linux content is still wry (`main.rs:1220`). RustKit content there would ride the same `PageThread` plus a `TaoHost` surface (wgpu takes a raw handle from tao on both).

---

## 5. Recommendation

**Option D**: a page thread per tab owning DOM, JS, style, layout and that page's network. A window thread owning windows, the ordered input queue, routing, presentation, the watchdog and native browser commands. The chrome document is another page thread. Delivery goes one content thread → per tab, and the VM interrupt is landed first.

**Why the others lose:**

- **C** cannot meet the goal. Boa terminates but cannot pause, so responsiveness under C requires killing any script longer than about 100 ms, which breaks real sites (YouTube's base module ran past 5 s). It also leaves load, layout and paint on the window thread. Its terminators are kept in D; its single thread is not.
- **B** keeps the browser controls alive, but one busy page stalls every tab. "Switch to another tab and use it" fails during a runaway until the interrupt fires. In D, B survives only as the first landing because it carries the protocol.
- **A as a single jump** is the right end state but a large unshippable step. D gets there in pieces that each leave the app working.
- **Not chosen: a separate process per tab.** It is the only way to *kill* a stuck native loop, and it gives a security boundary. It is not needed for responsiveness once T2 exists. D keeps every cross-thread message plain data, so a process split later only changes the transport (open question 4).

---

## 6. Migration

Each step lands alone and leaves the app working. They are ordered smallest-first where dependencies allow. Real-window checks extend `tools/real_window/driver.py`. Its README notes that input-posting checks have not run on a granted seat yet (`tools/real_window/README.md`, status 2026-10-05), so each step's input checks are also hand-test items until the driver's input path is proven.

New fixtures used below. **`h20_spin_after_load.html`** loads, paints green, and 3 s later starts a `setTimeout` callback that spins inside nested `forEach` and **calls the host on every iteration** (`document.getElementById`, the probe #616's tests use), so T1 alone can stop it. **`h20b`** spins after load in pure script: a bare `while(true){}` inside a `forEach` callback, with no host calls, so only T2 (M1) stops it. A second tab holds `h1_late.html`, which ticks once a second.

| # | step | what changes | proves it | size | Phase X order |
|---|---|---|---|---|---|
| **M0** | Deadline on every script entry | `set_execution_deadline` armed in `pump_live`, `fire_mouse`/`fire_key` dispatch and `execute_script`, with `LIVE_TURN_BUDGET` (open question 2); `run_jobs` keeps its count bound. Single thread still: this bounds the freeze, it does not remove it. | Engine tests: a timer callback spinning in `forEach` with host calls after load is stopped and recorded `OverBudget`; the page's other timers still run. Real window: `h20` (host calls) loads, then the window answers a chrome click within `LIVE_TURN_BUDGET` + 1 s of the spin starting. `h20b` is **not** expected to pass here; it is M1's proof. | S | any time; before everything else here |
| **M1** | Boa VM interrupt (T2) + `ThreadStamp`/`ViewControl` | Vendored `third_party/boa_engine` 0.22.0, patch ≈30 lines: `RuntimeLimits` gains `interrupt: Option<Arc<AtomicU64>>` and `turn: u64`, checked in `check_runtime_limits` (`vm/mod.rs:1017`) and `IncrementLoopIteration` (`loop_ops.rs:14`) as `word == turn \|\| word == ALL`. `ThreadStamp` and `ViewControl` (section 3) are introduced, with the `fetch_max` / `compare_exchange(ALL, 0)` rules and the publish/read order; `ExecutionDeadline` gains the `control` link and the turn cell; `JsRuntime::set_turn(id)`. Remove the stale `boa_gc`/`boa_parser` patches. | rustkit-js tests: `while(true){}`, a pure `forEach` spin and a recursion-free reducer all stop within 50 ms of the word being set **from another thread**; `try/catch` is not entered; the runtime is usable afterwards; **a word naming turn N does not stop turn N+1**; `ALL` stops every turn until cleared; **a `fetch_max(N)` after `ALL` leaves `ALL`**; a watchdog-read unit test that interleaves turn end/start between its two reads never acts on the new turn (loom or a stepped test). Benchmark: the parity board's script-heavy cases are within noise. Real window: `h20b` (no host calls) is stopped at `LIVE_TURN_BUDGET`. | S | any time |
| **M2** | Ordered, view-tagged input queue (#575) | One `InputEvent{seq, at, view, kind}` queue in `rustkit-viewhost/src/macos.rs` replaces the three statics; `RustKitContentView` carries its `ViewId`; `main.rs:2335-2447` drains in `seq` order and drops events for a missing view; adjacent-only coalescing. | Unit: click after a wheel burst is delivered after it; key typed before a click is delivered before it; a wheel queued for view 1 is dropped once view 1 is gone. Real window: `h6` scroll, then a tab switch mid-flick does not move the new tab. | M | before Phase X step 3 (the view host takes over this queue) |
| **M3** | Split `Engine` into `PageEngine` + `Presenter`, same thread | Presenter = viewhost + compositor + renderer + surfaces; PageEngine produces `FrameCommit`; `render` consumes commits; `assert_send::<FrameCommit>()`, `::<ResourceLoader>()` etc. as compile-time tests **on macOS**; one wgpu device per process. | Whole engine test suite; the 26-case parity campaign shows identical `diffPixels` A/B; the app draws the same frames. | L | before Phase X step 2 (the chrome document needs `PageEngine` without its own GPU device) |
| **M3b** | Cooperative load cancel | `nav_id` on loads; `ViewControl.cancel_nav` (cancel ids `<=`); `nav_superseded` (`lib.rs:9689`) also reads it; the same check between scripts in `run_page_scripts_with` (`lib.rs:3821`) and between subresource phases; in-flight fetches race a `Notify`; the load returns through its superseded path, never dropped (section 3, load cancellation). Single thread still. | Engine test: a load with a host-calling spin script, five more scripts and twenty slow subresources; another thread sets `cancel_nav` and `interrupt` at 200 ms; `load_url` returns within 1 s; none of the five later scripts ran; no subresource request started after the cancel (fixture log); `view.bindings` is present and a new navigation of the same view loads and runs script. | M | before M4 |
| **M4** | `PageThread` with one content thread (B shape) | `RustKitView` becomes a `PageHandle` (Sender, `Arc<ThreadStamp>`, `Arc<ViewControl>`, the window-assigned `nav_id` counter); queued `Navigate`s with `nav_id <= cancel_nav` are dropped. Load, live turns, input, eval and stop run on the content thread with one **persistent** tokio current-thread runtime: the live loop's `LiveFuture`s must be polled by the same runtime on every turn (`script_net.rs:141-142`), and a runtime per load (`webview_rustkit.rs:525`) goes away. The synchronous queries become messages: `link_at_point` (`main.rs:2139`), `has_focused_element` (`:2165`, `:2427`) and `form_submit_url` (`:2181`, `:2421`) turn into page-side decisions reported as `PageEvent::NavigateRequested` / `Focused`, and `handle_text_key`'s scroll fallback moves into the page thread. The window thread runs the watchdog and wakes through `EventLoopProxy`. Stop = `cancel_nav` + `interrupt`, then `PageCmd::Stop`. Still **one shared view**, so tab switch and close are still a `Navigate` (`main.rs:783-819`), now cancellable. Behind `HIWAVE_CONTENT_THREAD=1`, then default after one hand-test day. | Real window: (1) **`h19_spin` during load**: chrome clicks, typing in the URL bar and window drag answer within 100 ms throughout; **Stop** ends the load within 1 s (no further script or subresource requests in the fixture log); Stop pressed right after clicking a link on a busy page cancels that navigation too (its request never reaches the fixture server); (2) **`h20` after load**: Stop ends the spin within 1 s and the page's later timers still run; (3) **close the spinning tab**: it leaves the tab strip within 100 ms, the spin ends within 1 s, and the next tab's load begins within 1 s (request log). Every `h1`–`h16` check stays PASS. Tab switch to a retained frame is **not** claimed here (M5). | L | after M3b and Phase X step 1 (fewer cfg branches); before Phase X step 2 |
| **M5** | One view per tab, retained | Each tab owns a `ViewId`: its `ViewState` (DOM, JS, layout) lives on the content thread, and its NSView, surface and last `FrameCommit` live in the window thread's Presenter (2.4, 2.5). Tab switch shows/hides instead of `Navigate` (`main.rs:797-819`); background-tab cap with discard (open question 3). | Real window: switch A→B→A does not re-request A's page (fixture request log); form text typed in A survives the switch; the cap discards the oldest and it reloads on activation; **with `h20` spinning in A, switching to the `h1` tab B shows B's last frame within 100 ms** (B stays inert until A's spin is interrupted: one content thread). | M | after M4; any Phase X order |
| **M6** | One page thread per tab (A shape) | `PageThread` per tab, each with its own `ThreadStamp` and runtime (one `ViewControl` per view as before); shared `Arc<ResourceLoader>`/`FontLoader`; a new tab gets a new thread; abandonment after 2 s without an acknowledgement. | Real window: `h20` in tab A spinning; tab B (`h1`) **keeps ticking in the request log during the spin**; switch to B and click works; open a new tab and load a fixture page in it during the spin; a page held 20 s by the fixture server in tab A while tab B scrolls and runs find-in-page; Stop in A ends the spin; close A while spinning: the tab disappears at once and the thread exits or is reported abandoned. | L | after M5; before Phase X step 3 (the view host is built against the final shape) |
| **M7** | Chrome document as a page thread (= Phase X step 2) | `HIWAVE_RUSTKIT_CHROME=1`: the chrome HTML runs in a RustKit `PageThread` with `ChromeEvent`/`ChromeCmd`; native accelerators for stop/close/next tab/quit wired on the window thread. | With the flag: all M4/M6 real-window checks pass using **keyboard accelerators only**, and again by clicking the RustKit-drawn tab strip and stop button; a chrome script forced to spin (test hook) does not stop Cmd+W or page scrolling. | L | is Phase X step 2 |
| **M8** | `ViewHostMac` replaces `TaoHost` on macOS (= Phase X step 3) | `WindowHost` trait; macOS run loop, `WakeHandle`, window events, IME and the settings window move to `rustkit-viewhost`; main-thread markers. Windows and Linux keep `TaoHost`. | All earlier real-window checks on macOS without tao in the macOS dependency graph (`cargo tree -p hiwave-app --target aarch64-apple-darwin -i tao` is empty); Windows and Linux CI builds unchanged. | L | is Phase X step 3; requires M2, M4, M7 |
| **M9** (optional) | Render thread | Encode+present off the main thread using explicit-extent configure (2.4); the window thread only routes. | Wikipedia portal: input-to-scroll latency is measured before and after; no surface panic; resize has no tearing in the hand test. | M | after M8 |
| **M9b** (optional) | Predicted scrolling on the window thread | The presenter draws `committed offset + unacknowledged wheel deltas` (section 3A, scroll rule); commits carry `scroll_ack`. | Real window: with `h20` spinning in the front tab, a wheel flick moves its last frame within one frame; after the spin ends, a page `scrollTo` during a flick lands where the page put it plus the later deltas only. | M | after M6 |

Phase X step 1 (delete `webview-fallback`) is independent of M0–M3b and should land before M4. Phase X step 4 (wry leaves) comes after M7 has been default for a release and M8 has landed. On Windows and Linux, wry remains wherever they still use it for content.

---

## 7. Acceptance tests (user tasks)

Each is run in the built app on macOS, by hand and, where the driver can, by `tools/real_window`. Each names the step at which it first passes; before that step it is expected to fail.

1. **Runaway after load, then tab switch.**
   - Setup: open the `h1` tick page in tab 1 and the `h20` spin page in tab 2.
   - Wait until tab 2 starts spinning (its fixture beacons).
   - Click tab 1 within 1 s: it appears within 100 ms and keeps ticking.
   - First passes: shows within 100 ms at **M5**; keeps ticking during the spin at **M6**.
2. **Stop a runaway.**
   - In the spinning tab, press the stop button (and, separately, Esc / Cmd+.).
   - The spin ends within 1 s. The page stays on screen, scrollable and clickable, and its later timers still run.
   - First passes: **M4** (the stop button; Esc / Cmd+. need their menu items, M7 at the latest).
3. **Close a runaway tab.**
   - In the spinning tab, press Cmd+W (and, separately, the tab's close button).
   - The tab is gone within 100 ms, and the app's CPU returns to idle within 2 s.
   - First passes: **M4**.
4. **Runaway during load.**
   - Load `h19_spin`.
   - While it spins: type in the URL bar, open a new tab, navigate it to a fixture page, and drag the window. Every action responds within 100 ms.
   - First passes: typing and dragging at **M4**; the new tab's page loading during the spin at **M6** (until then it waits behind the spin on the one content thread).
5. **Pure-script spin.**
   - Repeat tasks 2 and 3 on `h20b` (`while(true){}`, no host calls).
   - Same results.
   - First passes: **M4** (needs M1, which lands earlier).
6. **Slow network does not freeze the browser.**
   - Load a page whose HTML the fixture server holds for 20 s.
   - Switch tabs, open the shelf, and use find-in-page in another tab during the wait.
   - First passes: switching and the shelf at **M5**; find-in-page in the other tab (it runs in that page) at **M6**.
7. **Input order and identity.**
   - On a page logging events: scroll then click, type then click. The page log shows the arrival order.
   - Flick-scroll tab A and switch to tab B mid-flick: B does not move.
   - First passes: **M2**.
8. **Everyday pages unchanged.**
   - The `h1`–`h16` checks PASS.
   - The 26-case parity campaign is identical A/B.
   - Wikipedia, YouTube and eBay load and are usable.
   - Holds at every step.
9. **Phase X shape, once M7 and M8 are in.**
   - Repeat tasks 1–7 with `HIWAVE_RUSTKIT_CHROME=1` on a macOS build whose dependency graph has no tao.

---

## 8. Open questions for the project owner

1. **Carry a patched `boa_engine` 0.22 for the VM interrupt (M1)?**
   - Without it, pure script, such as the 8 s host-free stretch PR #616 measured on ebay's module graph, cannot be stopped short of abandoning the thread.
   - *Recommended:* yes. Vendor it under `third_party/boa_engine` with the roughly 30-line patch and a test. Offer it upstream as an interrupt hook. Drop the stale `boa_gc`/`boa_parser` patches in the same PR.
2. **Script budget after load (`LIVE_TURN_BUDGET`)?**
   - *Recommended:* 5 s per live turn or handler, interrupted. The load budget stays at 15 s. Once M4 lands, show a "this page is busy — Stop" bar on the window thread after 2 s of one turn, without killing the script. The user's Stop is always immediate.
3. **How many tabs keep a live page thread (M5/M6)?**
   - *Recommended:* the active tab plus the 7 most recently used. Discard the rest to (URL, scroll, last frame) and reload on activation. Fold this into Tab Decay instead of a separate setting.
4. **Is a process per tab in scope?**
   - *Recommended:* not now. Threads plus T1/T2 meet the responsiveness goal. Keep every `PageCmd`/`PageEvent` plain data (no `Arc` to page state) so a process split stays a transport change. Revisit when the security work asks for site isolation.
5. **Should the RustKit chrome run on its own page thread, or on the window thread (Phase X step 2)?**
   - *Recommended:* its own page thread, with stop, close and switch also available as native accelerators on the window thread. The window thread then never runs JS or layout, and that is the invariant the watchdog relies on.
