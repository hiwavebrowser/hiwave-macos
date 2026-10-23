# Live feed exceptions after 62d31185

Head under test: `62d31185` (`cloud/reddit-feed-live`) plus the console-log switch from #718 (`HIWAVE_CONSOLE_LOG=1`). The front page still does not fill in. Within about 2 seconds of the static bundles running, the log had 12 uncaught exceptions and 8 unhandled rejections. A reload then re-requested the challenge document, held the window thread for 12.7 seconds, and afterwards one feed post rendered. The other rows stayed blank.

The public concat bundle named in the first stack (`concat?chunks=BvOWmaNPEJ,DZDBx0nCO6,Dyg3q1gB6W`) was not readable from here: `www.redditstatic.com/js/concat` and the shreddit concat paths returned 404, and `www.reddit.com` returned 403. The call site at line 101, column 71336 was not read. The notes below are from the engine (Boa 0.22 and the rustkit bindings) and from the messages themselves.

## Ranked causes

1. **The window thread runs the whole reload, and the script budget cuts async work off.** Separate from the exceptions, and the reason a reload freezes the window. See the stall section. The `async function was interrupted` rejection in `STICKY_CANARY` is this budget, not a missing built-in.
2. **`TypeError: not a callable function` inside the client bundle**, invoked from the challenge page. Boa raises that when a call target is not a function. The bundle was not available to name the callee. Not fixed.
3. **`Failed to construct 'URL': Invalid URL`.** `location` had no stringifier, so `new URL(path, location)` parsed the base as `"[object Object]"`. Fixed.
4. **`RangeError: Invalid time value` (four times).** Boa's `Date.parse` accepted only an ISO fraction of exactly three digits and only a `+HH:MM` offset. A timestamp such as `2024-10-16T18:22:11.123000+0000` became Invalid Date, and `toISOString` / `toJSON` then throw this RangeError. Fixed in the vendored date parser.
5. **`MediaError is not defined`.** The HTML media-error interface was missing. Fixed with a non-constructable stub and the four code constants. `HTMLMediaElement.error` stays `null` (nothing plays).
6. **Two reCAPTCHA SDK load timeouts.** The challenge page's own wait for the SDK expired. That is a script that never became ready (blocked, slow, or cut off), not a constructor we should invent. Not fixed.
7. **A dynamic `import()` network error.** A module fetch that the shield or the network refuses is delivered as `"network error"`, and the import promise rejects. A fetch still in flight when the turn's module deadline passes is `"script budget spent"` instead. Not fixed.
8. **A null-to-object conversion inside the reCAPTCHA script.** Boa's message is `cannot convert null or undefined to Object` (`require_object_coercible`). The SDK called an object operation on null or undefined, which fits an SDK that did not finish loading. No single missing built-in is identifiable without a column in that script. Not fixed.
9. **Reload requested the challenge URL.** The chrome reload loads `current_url`. A script navigation (`location` assignment, `location.replace`, a meta refresh) is committed through `UserEvent::Navigate`, and that replaces `current_url`. Reload then fetches the challenge document again rather than the URL that was typed. Not fixed.

## What each message is

### `not a callable function`

Boa's call opcodes (`third_party/boa_engine/src/vm/opcode/call/mod.rs`) throw `TypeError: not a callable function` when the callee is not a function. A missing binding is a different error (`MediaError is not defined`). This one means the bundle had a value and called it.

The stack is the concat bundle at `101:71336`, called from the challenge page at `6:1029`. The challenge page's inline script called into the client bundle, and the throw is inside the bundle. `Intl`, `URLSearchParams`, `structuredClone`, `requestIdleCallback`, `queueMicrotask`, `CustomElementRegistry`, and `AbortSignal.timeout` are all present, so this is not one of those names being absent. It is some other call target in that bundle (a method that is missing, or a value that is not a function). The chunk ids were not on the public static host when this was checked, so the callee is unnamed.

### `Failed to construct 'URL': Invalid URL`

`web_url.js` throws that when the `url` crate cannot parse the input against the base. The crate already strips tabs and newlines and treats `\` as `/` in special URLs. The gap was `Location`.

HTML says `Location`'s stringifier is its href. `new URL(relative, location)` and `String(location)` use it. Our `location` was a plain object with an `href` accessor and no `toString` / `Symbol.toPrimitive`, so the base string was `"[object Object]"` and the constructor threw. Pages pass `location` as the base constantly. `location.toString`, `valueOf`, and `Symbol.toPrimitive` now return the href.

An absolute URL with an unparseable base still throws. That matches the URL constructor: a base that does not parse fails before the input is used.

### `RangeError: Invalid time value`

`Date.prototype.toISOString` (and `toJSON`, which calls it) throws `Invalid time value` when the time is `NaN`. `Intl.DateTimeFormat.format` in `web_intl.js` throws the same message for a non-finite time. Four of these in two seconds is a handful of timestamps that did not parse, then a conversion to an ISO string.

Boa 0.22's `parse_date` (`third_party/boa_engine/src/builtins/date/utils.rs`) accepted the spec form `YYYY-MM-DDTHH:mm:ss.sssZ` only when the fraction was exactly three digits, and an offset only as `+HH:MM`. These all returned `NaN`:

| Input | Why it failed |
|---|---|
| `2024-10-16T18:22:11.123000+0000` | six fraction digits, then `+HHMM` |
| `2024-10-16T18:22:11.1234Z` | four fraction digits |
| `2024-10-16T18:22:11.1Z` | one fraction digit |
| `2024-10-16T18:22:11+0000` | offset with no colon |
| `2024-10-16 18:22:11Z` | space instead of `T` |

The date-time grammar allows one or more fractional digits. Browsers also accept a basic `+HHMM` offset and a space separator. The parser now does: the first three fraction digits are the milliseconds (fewer are padded, the rest are dropped), and an offset may be `+HH:MM`, `+HHMM`, or `+HH`. `T`, `t`, and a space all start the time. `Date.parse('not-a-date')` is still `NaN`. The existing `2020-06-08T09:16:15.779-07:30` case still parses.

`MM/DD/YYYY` and month names are still rejected. Those are implementation-defined, and nothing in this log says the page needed them.

### `async function was interrupted` (`STICKY_CANARY`)

This string is not from the page. The vendored await opcode, when the watchdog's uncatchable interrupt lands inside an async function, rejects that function's promise with `async function was interrupted` (`third_party/boa_engine/src/vm/opcode/await/mod.rs`). The same path abandons an interrupted module body so a later import does not hang.

The app arms that watchdog (`interrupt_scripts_at_budget`). One budget covers everything the turn runs, including a promise job that resumes an async function:

- During the load, the budget is `script_budget_ms`, 15 seconds (`LIVE_SCRIPT_BUDGET_MS` in `webview_rustkit.rs`). `run_page_scripts` calls `begin_script_turn` with that budget.
- After the load, each live turn gets `live_turn_budget_ms`, 5 seconds (`arm_live_turn` in `pump_live`). Timers, response handlers, and inserted scripts share it. An async function resumed in that turn is stopped with the turn.

`STICKY_CANARY` is a module in the client bundle (filter lists refer to it by that name). Its async function was resumed on a turn whose budget expired, and the rejection was reported as unhandled. The function does not continue on the next turn. That is the interrupt design, not a missing API. It was not changed here: raising the budget, or resuming an interrupted async function on a later turn, is a scheduler change, not a small gap.

### `MediaError is not defined`

A reference to the global, not a call of a non-function. HTML defines `MediaError` with `MEDIA_ERR_ABORTED` (1), `MEDIA_ERR_NETWORK` (2), `MEDIA_ERR_DECODE` (3), and `MEDIA_ERR_SRC_NOT_SUPPORTED` (4). The interface is now on `window`, the constants are on the constructor and the prototype, `new MediaError()` throws `TypeError: Illegal constructor`, and a media element's `error` is `null`.

### reCAPTCHA load timeouts

Two timeouts from the challenge page waiting on the reCAPTCHA SDK. The SDK script did not become ready in time. Possible contributors already in this engine: the script was blocked or slow, its `load` never fired, or the turn was stopped first. Faking a `grecaptcha` object would send the page down a path that still cannot complete. Not added.

### Dynamic import network error

`import()` is fetched in `pump_modules` (`script_net.rs`). A denied fetch becomes the string `network error` and the import promise rejects. An unparseable module URL is `unparseable module url`. A fetch that is still going when the turn's module deadline hits is `script budget spent`, and the import is asked again on a later turn only if the caller retries. A network rejection does not retry by itself. This matches a module the shield or the network refused. Not changed.

### Null to object, inside reCAPTCHA

`JsValue::require_object_coercible` throws `TypeError: cannot convert null or undefined to Object`. That is `Object` operations and several built-ins on `null` or `undefined` (`Object.keys(null)`, a method borrowed and called with a null `this`, and similar). The stack was in the reCAPTCHA script, which also timed out loading. The conversion is what that script does when a value it expected was null. It is not evidence for a particular missing interface.

## Reload: the challenge URL, then 12.7 seconds, then one post

The chrome Reload action calls `RustKitView::nav_reload`, which loads `current_url` on the window thread (`load_url_blocking`: a current-thread runtime, `block_on` of `Engine::load_url`). The window does not process input or paint until that returns. 12.7 seconds is that call. It is under the 15 second load script budget, so the budget does not have to have fired for the window to be frozen the whole time. Fetches and scripts of the reload all run there.

`current_url` is the last URL the app committed. The address the user typed stays only until something navigates. Script navigation is taken after each live turn (`take_script_navigation`) and dispatched as `UserEvent::Navigate`, which updates the tab and `current_url` before loading. `location.href =`, `location.replace`, and a meta refresh all go through that. `history.replaceState` updates the document's `location` only; it does not change `current_url`. The reload request was the challenge URL, so a real navigation to that URL had been committed. The document URL the engine stores is the URL it was asked to load, not a later HTTP redirect's final URL. Reloading therefore repeats the challenge navigation.

After that load returned, one feed post was on screen and the other rows were still the blank skeleton. One chunk of the feed ran. The rest of the rows need the scripts that threw or were interrupted above: a non-callable in the client bundle, a URL or date throw while a post is built, the reCAPTCHA path on the challenge document, or `STICKY_CANARY` being rejected mid-async. Those are still open, apart from the URL stringifier, the date parser, and `MediaError`.

## What was fixed

- `Location` stringifies to its href, so it can be the base of `new URL`.
- `Date.parse` accepts a variable-length fraction, a `+HHMM` offset, and a space or `t` before the time. `toISOString` no longer throws on those timestamps.
- `MediaError` exists, with the four code constants. It cannot be constructed. `media.error` is `null`.

## What was left

The callable in the unread bundle, the reCAPTCHA timeouts, the failed dynamic import, the null-to-object throw inside that SDK, the reload target following the challenge navigation, and the window-thread load plus the async interrupt. The 5 second live-turn budget and the 15 second load budget were not changed.
