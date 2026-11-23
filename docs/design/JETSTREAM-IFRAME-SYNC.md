# JETSTREAM-IFRAME-SYNC: a frame's about:blank document at insertion, and document.open/write

Status: Option B approved by Atlas (frames author) on #1434 with six changes, folded in below (marked **[R1]**…**[R6]**), 2026-11-23. No code yet.
Plan row: JETSTREAM-IFRAME-SYNC (stress suites rank first).

## Why

JetStream 2.2 gets no score: it fails its first test
(`docs/diagnostics/2026-11-21/STRESS-TRUTH-2026-11-21.md`, "JetStream 2.2"). Each benchmark does:

```js
magic.contentDocument.body.innerHTML = "<iframe id=\"magicframe\" frameborder=\"0\">";
var magicFrame = magic.contentDocument.getElementById("magicframe");
magicFrame.contentDocument.open();
magicFrame.contentDocument.write("<!DOCTYPE html>...<script>...</script><script src=\"blob:...\"></script>...");
```

This needs two things, all inside one script:

1. a frame's `about:blank` document, available as soon as the frame is inserted, nested one level too;
2. `document.open()` / `document.write()` on it, running the written scripts as the parser reaches them.

## What Chrome does

Receipt: `docs/diagnostics/2026-11-23/chrome148_iframe_write.json`, from `tools/parity_oracle/iframe_write_log.mjs`. Chrome for Testing 148.0.7778.216 (linux64).

- **Before insertion,** `contentDocument` is `null`.
- **Right after `appendChild`,** whether there is no `src` or `src="about:blank"`, it is a document:
  - URL `about:blank`, `readyState` `complete`;
  - `<html><head></head><body></body></html>`;
  - `contentWindow.document` is the same object.
- **A frame made by `innerHTML` inside that document** has its own `about:blank` document right away too.
- **`open()`** empties the document: `documentElement` and `body` are `null`.
- **`write(markup)`** parses as it goes and runs each inline script when the parser reaches it, synchronously, inside the `write` call:
  - the first script sees `readyState` `loading`;
  - the second already sees the `<p>` written before it;
  - a second `write` runs its script too.
- **`close()`** finishes parsing; `readyState` becomes `complete`.

## Where RustKit is

Today, with nested contexts on (`HIWAVE_NESTED_CONTEXTS`):

- `web_frames.js` only queues a request (`__rustkit_frame_request` → `FrameRequest`).
- The engine drains the queue after the inserting script returns (`Engine` frame drain, rustkit-engine `lib.rs`). For `about:blank` it:
  - parses the fixed blank markup;
  - calls `DomBindings::new_child()` (a new realm of the same `JsRuntime`, with the whole page environment installed by `build_in`);
  - calls `set_document`, `set_location`, `set_dimensions` and `enable_net_bridge`;
  - calls `attach_frame`, which runs bookkeeping scripts in the parent and the child (`__rk.frames.attach`, `__rk.becomeFrame`);
  - creates an offscreen view.
- Until then, `contentDocument` is `null`. That is the throw.
- `Document.prototype` has neither `open` nor `write`. The only `write` is a no-op on the legacy stub document.

**The constraint: no reentrancy.** The frame request arrives while the parent's script is running. That script runs inside `JsRuntime::evaluate(&mut self)`, which holds the whole runtime: the realm list, the per-realm host objects, the module host, the deadline, the blob store. `DomBindings` reaches the runtime through `Rc<RefCell<…>>`, which is borrowed for the whole evaluation. And host functions see neither: `HostFunction = Fn(&[JsValue]) -> JsValue`; the wrapper drops the `&mut Context` that Boa passes. So `new_child` / `attach_frame` cannot run inside the request call as they are.

## Rejected: a placeholder document in the parent realm

`contentDocument` could answer `document.implementation.createHTMLDocument('')` in the parent realm until the engine attaches the real child.

It's cheap, but `document.write`'s scripts would then run in the **parent's** realm. JetStream relies on one iframe per benchmark to keep each benchmark's globals apart. A placeholder makes them collide, and the child's `window.onerror` / `top` relationships break.

## Option A: build the child realm inside the call, by splitting `JsRuntime`

Split `JsRuntime` into:
- the Boa `Context`;
- a `RuntimeState` in `Rc<RefCell<…>>` (realms, realm hosts, module host, deadline, blobs, sources, …), borrowed only briefly.

Give the realm-building operations a context-level form, `fn …(state: &RefCell<RuntimeState>, ctx: &mut Context)`. Add a host-function variant that receives Boa's `&mut Context`. Then the request call can run `create_realm`, the `build_in` installs, `set_document` / `set_location`, and the attach handoff, then queue a "prebuilt" request for the engine to adopt.

- **For:** general, with no depth limit, and the same machinery serves any future synchronous host operation (sync XHR, `window.open`, …).
- **Against:** a core refactor of rustkit-js (`JsRuntime` has on the order of a hundred methods over `self.context`) and of `DomBindings::build_in` (185 lines of `runtime.…` calls). A large blast radius for one benchmark row.

## Option B (recommended): pre-built spare child realms

Build each child **before** it's needed, at a drain, where nothing is running. Inside the call, *attaching* it needs only JS calls on objects the parent already holds.

1. **The spare.**
   - **[R1] Built lazily, never at page load.** A document's spare is built:
     - at the **first frame drain of that document** (its first iframe actually drained); and
     - when a document is **attached as a frame**. The frame's own spare is built at its adoption, so that a script inside it can make a frame synchronously.
   - Pages with nested contexts on but no iframes pay nothing.
   - **JetStream 2.2's driver** (browserbench.org/JetStream2.2/JetStreamDriver.js) only looks the outer frame up (`document.getElementById("magic")`); script never creates it. So `magic` comes from the page's markup and is drained at load, before any benchmark runs. Its adoption builds the spare **in `magic`'s document**, and that spare is what `magic.contentDocument.body.innerHTML = "<iframe id=\"magicframe\">"` takes, synchronously. (The fetched index.html text lost its tags, so the `<iframe id="magic">` tag itself is inferred, not quoted.)
   - After a spare is used, the drain builds the next one, as the bullets below say.
   - **The spare's contents:** the engine builds it with:
     - `new_child()`;
     - `set_document` of a fresh `about:blank` document (`<html><head></head><body></body></html>`);
     - `set_location(about:blank)`.
   - It is stored on the parent (`DomBindings`), with its global and its `__rk.becomeFrame` reachable as JS objects from the parent's `__rk.frames`.
   - **Depth:** with the lazy rule above, JetStream needs one level in a single statement (`magicframe` inside the already-adopted `magic`). A spare taken from one level still brings its own spare, built with it, so a frame made inside a just-attached frame in the same statement works too. The drain refills both. Deeper synchronous nesting falls back (point 4).
   - **[R2] A spare belongs to one document.**
     - Both are discarded when that document navigates (a new `nav_id`, the same test that already drops a navigated document's frames) or its origin changes.
     - A spare is never handed across origins: the take checks the spare's owner document is the inserting document, and its origin is the same.
     - Its clocks (`performance.timeOrigin`, `document.timeline`) are set **at adoption**, not at build, so a spare built 10 s earlier doesn't report a 10 s-old clock. Adoption re-runs the time-origin setup (`clock_origin`) that `build_in` does today.
2. **At insertion** (`web_frames.js` `navigate`, for an initial `about:blank`, same-origin by definition, and with no `sandbox` attribute):
   - take the parent's spare in JS: the same handoff as `attach_frame`, as plain calls (`__rk.frames.attach(node, spareGlobal)`, then the spare's `__rk.becomeFrame(up, top, el)`);
   - mark the request **prebuilt** with the spare's id.
   - `contentWindow` / `contentDocument` answer at once. The blank `load` keeps today's rule (`INITIAL` / `QUIET`, the `heardAt` record).
3. **At the drain,** for a prebuilt request, the engine **adopts** the spare instead of calling `new_child`:
   - `set_dimensions` and `enable_net_bridge`;
   - the offscreen view;
   - the frame record;
   - then it builds the next spare.
   - `attach_frame` is not called again: the attach already happened. A guard checks the frame still holds that window. If not (the frame left in the same script), the child is dropped as today.
4. **No spare:** deeper than the spare chain, a document whose first frame drain hasn't happened yet, or a spare already taken this script. Fall back to today's path: `null` until the drain, with the stated limit kept.
5. **#1421's module map:** maps are keyed by (realm index, URL) (`ModuleHost::entries`). A spare is a realm with an index like any child, so it gets its own map. Taking it changes nothing; dropping it drops its map entries as `drop_child` does today.

- **Cost:** one spare (plus its own) per document that has frames, built off the inserting script's path (at drains). `build_in` installs the whole page environment, so this is measurable.
- **[R5] The cost gate, decided now:**
  - The code PR measures build time and memory per spare pair.
  - It also measures frames-on page load on the parity campaign and the chrome board: median of 3 runs, spares on vs off.
  - If page load regresses by **more than 5%**, the spare build moves off the load path, or we revisit Option A.
- **Flag off: no spares, no cost.**

Recommendation: **B.** It's contained to rustkit-bindings (`web_frames.js`, `DomBindings`) and the engine's frame drain, and it needs no rustkit-js core change. A can come later if another synchronous host operation needs the same thing.

## document.open / write / close

These are needed under either option. Today they don't exist on `Document.prototype`.

- **A host function that receives `&mut Context`.** This is a new variant of `register_host_function`; Boa already passes the context, and the wrapper drops it. It's needed because the written scripts must run in the **document's** realm, synchronously, inside the `write` call made from the parent's script.
- **`open()`:**
  - removes the document's children (the Rust DOM, through the existing mutation path, so the JS wrappers and layout see it);
  - sets `readyState` to `loading`;
  - starts a **write parser** for that document.
  - Out of scope: `open()` on the top document with a navigation, and `open(type, replace)` arguments beyond Chrome's no-op cases.
- **`write(...strings)`:** appends to the write parser's input and parses as far as it can.
  - Each completed element is inserted into the live document.
  - At a completed inline `<script>`, the write host function enters the document's realm, runs the script, and restores the realm.
  - A `<script src>` with a `blob:` URL is read synchronously from the runtime's blob store and run the same way. That's what JetStream needs: parser-blocking, and blobs are local.
  - Any other `src` is out of scope for this row: stated as a limit, skipped with a console note, not faked.
  - Each written script runs as a **classic script**: Boa `Script` parse and evaluate, with the document's realm entered. Not an indirect `eval`: that would scope top-level `let`/`class` to the call and break scripts that share declarations, as JetStream's do.
  - The written scripts run under the running script's deadline. They are part of the same task, so the 30 s live stop still applies.
  - **[R3] Errors.** An uncaught exception in a written script is reported to the **frame's** `window.onerror` / `error` event, as Chrome does (JetStream sets `window.onerror = top.currentReject` there).
    - The write host function catches it. It calls the frame realm's own `__rk.reportError(e)` as a plain JS function, reached through the frame's global, which the parent already holds. That's the path callbacks use today (web_components.js, web_mutation_observer.js, …), and it needs no `RuntimeState`.
    - The error then goes on to the next written script, as in Chrome.
    - **Our logs carry shapes, not source:** the frame's node id, the script's ordinal in the write, its length, and the error's class and message, through `tracing`. The runtime's source registry (`ScriptSources`) isn't reachable from the call, so written scripts don't appear in its stack-trace source map. That's stated as a limit.
  - **[R4] Microtasks.** Inside `write` the JS stack isn't empty, so, as in Chrome, **no microtask checkpoint runs between written scripts.** A promise reaction queued by written script 1 has not run when written script 2 starts. It runs at the outer script's checkpoint. The write host function never drains jobs; a test pins it.
- **`close()`:** ends the input, flushes the parser, and sets `readyState` to `complete`.
- **The write parser:** rustkit-html's tree builder fed incrementally, or, if it can't be fed incrementally, a buffer that parses whole fragments up to each `</script>`. I'll check rustkit-html before coding and say which in the code PR. The order guarantees must match Chrome's receipt either way.

## Flag off (nested contexts off)

There is no child realm, and that doesn't change.

- `contentDocument` will answer an **inert** `about:blank` document at insertion: `createHTMLDocument('')`-like, URL `about:blank`, `readyState` `complete`, in the parent realm. So `contentDocument` is never `null` after insertion, as Atlas asked.
- `open` / `write` / `close` parse into it, but **run no scripts**: flag off, frames have no browsing context to run them in. This is stated as a limit.
- So JetStream still has no score with the flag off; with it on, it should run its 59 non-wasm tests.
- **[R6] This changes what real-site scripts see.** Ad and embed code often checks `iframe.contentDocument` and then writes into it; it would now find a document where it found `null`. The code PR runs the **19-site A/B** (develop vs the branch, frames off). Any mover is judged against Chrome by hand, and the PR calls this out.

## Red tests (before code; Chrome 148 numbers from the receipt)

rustkit-engine, `--features headless`:

1. **`frames_sync_blank_tests::the_blank_document_is_there_right_after_insertion`** (flag on): an appended iframe with no `src`, and one with `src="about:blank"`:
   - `contentDocument` is non-null, URL `about:blank`, `readyState` `complete`, outerHTML `<html><head></head><body></body></html>`;
   - `contentWindow.document === contentDocument`;
   - `contentDocument` is `null` before insertion.
2. **`…::a_frame_made_inside_a_frame_has_its_document_at_once`** (flag on): JetStream's `innerHTML` pattern, depth 2.
3. **`…::the_blank_document_is_there_with_frames_off`** (flag off): non-null and inert, as above.
4. **`document_write_tests::write_runs_inline_scripts_as_the_parser_reaches_them`** (flag on): the receipt's sequence. `open()` empties; `write` gives `s1:loading`, then `s2:true`; a second `write` gives `s3`; `close()` gives `readyState` `complete`. The scripts run in the frame's realm: a global they define is on the frame's window, not the parent's.
5. **`…::a_written_blob_script_runs_before_the_markup_after_it`** (flag on).
6. **`…::two_frames_keep_their_globals_apart`** (flag on): each benchmark's isolation.
7. **`…::frames_off_write_runs_no_script`** (flag off).
8. **[R4] `…::no_microtask_checkpoint_between_written_scripts`** (flag on): written script 1 queues a promise reaction that sets a flag; written script 2 records the flag as unset. After the outer script, the flag is set.
9. **[R2] `frames_sync_blank_tests::a_spare_taken_after_the_parent_navigated_is_never_used`** (flag on): build a spare, navigate the parent, insert a frame. The frame is never the old document's spare (its document is fresh, of the new document's origin, with a new time origin).

Plus the stress check: JetStream with frames on runs past its first test. That's a receipt in the code PR, not a unit test.

## Limits, stated

- Synchronous frames go one spare chain deep (enough for JetStream); deeper nesting in one statement falls back to today's late attach.
- Written scripts don't appear in the runtime's source registry (their logs carry shapes).
- `write` runs inline and `blob:` scripts only.
- No top-document `open()` navigation.
- Flag off runs no written script.
- WebAssembly (5 of JetStream's 64 tests) and Workers are separate rows.
