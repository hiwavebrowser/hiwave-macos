# Web API census: 2026-10-03

A measurement of the JavaScript/DOM surface that a page script sees in RustKit's
bindings (`rustkit-bindings` on top of `rustkit-js`/Boa). This is measurement
only. No engine code changed.

- **develop SHA measured:** `91fa24e8a467591b529fd628b7934a988348ef3b`
- **Where it ran:** a Linux cloud pilot session. No macOS gates and no GPU or engine crates were run.
- **Probe:** `crates/rustkit-bindings/tests/census/web_api_census_probe.js`, driven by
  `crates/rustkit-bindings/tests/web_api_census.rs`
- **Raw data:** `docs/census/web_api_census.json` (one record per probed API)

## How to regenerate

```bash
git checkout 91fa24e8a467591b529fd628b7934a988348ef3b   # or any later develop
WEB_API_CENSUS_OUT=$PWD/docs/census/web_api_census.json \
  cargo test -p rustkit-bindings --test web_api_census -- --nocapture
python3 scripts/census/web_api_census_tables.py docs/census/web_api_census.json
```

The test prints `census: total=… works=… broken=… missing=…` and every row that
does not work. The Python script produces the Totals and Per-area sections below.

## Method

**Harness.** The probe uses the same setup as rustkit-engine for a page loaded
from a URL: `DomBindings::new(JsRuntime::new())`, `set_document(parse_html(…))`,
`set_location("https://census.test/start/index.html?a=1#top")`,
`set_dimensions(1280, 800)` and `enable_net_bridge()`. It then evaluates the probe.
To let asynchronous work settle, it calls `run_timers(60000, 10000)` five times
and evaluates a no-op script after each call, which runs pending promise jobs.
The fixture page holds a div, a list, a link, an img and a form with an input,
a label and a select.

**Classification** of each API:
- **MISSING:** the API's path resolves to `undefined`. For members that live on
  instances, the path is resolved on a fresh instance (`@div.attachShadow`
  means `document.createElement('div').attachShadow`). This means a method
  installed on instances rather than on the prototype still counts as present.
- **BROKEN:** the API is present, but its one smoke call threw, returned the
  wrong result, or (for callback or Promise APIs) never settled after the
  timers were drained.
- **works:** the API is present, and one small smoke call returned the checked
  result.

**Caveats. Read these before quoting numbers.**
1. A single smoke call shows that basic use works. It does not show spec
   conformance. Many "works" rows would still fail detailed tests (see the WPT
   section below).
2. 36 rows, marked `*`, can only check that a member is callable. Calling them
   for real needs a network, a user gesture, a GPU or another window. 7 of
   those 36 count as works.
3. **Selector matching is not the engine's.** The engine injects its cascade
   matcher through `set_selector_matcher`, and that matcher is `pub(crate)` in
   rustkit-engine, which does not build on Linux. This harness therefore uses the
   bindings' fallback matcher (a single tag, `#id` or `.class`). The two
   "complex/attribute selector" rows are BROKEN *in this harness only* and
   are left out of the ranking below. Real `querySelector` behaviour inside the
   engine was **not measured**.
4. There is no network. `fetch` and `XMLHttpRequest` were measured only up to
   queueing a request, and through `Request`/`Response`/`Headers`. Real responses
   were **not measured**.
5. Some areas have several rows (for example, MutationObserver has three).
   The counts are therefore probe rows, not distinct interfaces.

## Totals

- APIs probed: **444**
- works: **255** (57.4%)
- broken: **28** (6.3%)
- missing: **161** (36.3%)
- of which presence-only smoke checks (marked `*`): 36 (7 counted as works)

| Area | Probed | Works | Broken | Missing |
|---|---:|---:|---:|---:|
| Window & globals | 29 | 24 | 0 | 5 |
| Timers & scheduling | 11 | 9 | 0 | 2 |
| JavaScript builtins | 52 | 49 | 2 | 1 |
| Intl | 10 | 0 | 0 | 10 |
| Document | 51 | 21 | 1 | 29 |
| Node & tree mutation | 28 | 23 | 3 | 2 |
| Element | 55 | 31 | 3 | 21 |
| HTML elements | 30 | 7 | 0 | 23 |
| Events | 27 | 21 | 2 | 4 |
| Observers | 10 | 3 | 6 | 1 |
| Web Components | 7 | 5 | 1 | 1 |
| Networking | 21 | 18 | 0 | 3 |
| URL & encoding | 11 | 10 | 1 | 0 |
| Blob, File & FormData | 10 | 7 | 1 | 2 |
| Storage | 8 | 3 | 1 | 4 |
| Location, History & Navigator | 25 | 12 | 2 | 11 |
| CSSOM & view | 12 | 4 | 3 | 5 |
| Parsing & serialization | 6 | 0 | 0 | 6 |
| Range & Selection | 10 | 0 | 2 | 8 |
| Crypto & performance | 9 | 6 | 0 | 3 |
| Console | 6 | 2 | 0 | 4 |
| Workers & messaging | 4 | 0 | 0 | 4 |
| Graphics & media | 12 | 0 | 0 | 12 |
| **All** | **444** | **255** | **28** | **161** |

## Per-area results

### Window & globals (24/29 work)

| API | Status | Detail |
|---|---|---|
| window | works |  |
| self | works |  |
| globalThis | works |  |
| window.top | works |  |
| window.parent | works |  |
| window.frames | works |  |
| window.innerWidth | works |  |
| window.innerHeight | works |  |
| window.devicePixelRatio | works |  |
| window.scrollX/scrollY | works |  |
| window.pageYOffset | works |  |
| window.scrollTo | works |  |
| window.scrollBy | works |  |
| window.screen | works |  |
| window.open `*` | works |  |
| window.alert `*` | works |  |
| window.confirm `*` | works |  |
| window.postMessage | MISSING |  |
| window.addEventListener | works |  |
| window.onerror (property) `*` | MISSING |  |
| window.getSelection | works |  |
| window.name | works |  |
| window.origin | works |  |
| window.isSecureContext | works |  |
| window.frameElement | MISSING |  |
| window.visualViewport | MISSING |  |
| atob | works |  |
| btoa | works |  |
| reportError `*` | MISSING |  |

### Timers & scheduling (9/11 work)

| API | Status | Detail |
|---|---|---|
| setTimeout | works |  |
| clearTimeout | works |  |
| setInterval | works |  |
| clearInterval | works |  |
| requestAnimationFrame | works |  |
| cancelAnimationFrame | works |  |
| requestIdleCallback | works |  |
| cancelIdleCallback | works |  |
| queueMicrotask | works |  |
| scheduler.postTask | MISSING |  |
| MessageChannel | MISSING |  |

### JavaScript builtins (49/52 work)

| API | Status | Detail |
|---|---|---|
| Promise | works |  |
| Promise.all | works |  |
| Promise.allSettled | works |  |
| Promise.any | works |  |
| Promise.withResolvers | works |  |
| async/await | works |  |
| Map | works |  |
| Set | works |  |
| WeakMap | works |  |
| WeakSet | works |  |
| WeakRef | works |  |
| FinalizationRegistry | MISSING |  |
| Symbol | works |  |
| Proxy | works |  |
| Reflect | works |  |
| JSON | works |  |
| BigInt | works |  |
| ArrayBuffer | works |  |
| Uint8Array | works |  |
| Float64Array | works |  |
| DataView | works |  |
| SharedArrayBuffer | works |  |
| Atomics | works |  |
| Array.prototype.flat | works |  |
| Array.prototype.at | works |  |
| Array.prototype.findLast | works |  |
| Array.prototype.toSorted | works |  |
| Array.from | works |  |
| Object.fromEntries | works |  |
| Object.hasOwn | works |  |
| Object.groupBy | works |  |
| String.prototype.replaceAll | works |  |
| String.prototype.padStart | works |  |
| String.prototype.normalize | works |  |
| String.prototype.localeCompare | works |  |
| RegExp named groups | works |  |
| RegExp lookbehind | works |  |
| RegExp unicode property escapes | works |  |
| Date | works |  |
| Date.prototype.toLocaleDateString | BROKEN | threw: Function Unimplemented |
| Number.prototype.toLocaleString | BROKEN | result check failed (got false) |
| Math | works |  |
| globalThis.eval | works |  |
| Function constructor | works |  |
| Generators | works |  |
| Async iteration (for await) | works |  |
| Optional chaining / nullish | works |  |
| Classes with private fields | works |  |
| Error.cause | works |  |
| AggregateError | works |  |
| Array.prototype.includes | works |  |
| structured string escape (encodeURIComponent) | works |  |

### Intl (0/10 work)

| API | Status | Detail |
|---|---|---|
| Intl | MISSING |  |
| Intl.DateTimeFormat | MISSING |  |
| Intl.NumberFormat | MISSING |  |
| Intl.Collator | MISSING |  |
| Intl.PluralRules | MISSING |  |
| Intl.RelativeTimeFormat | MISSING |  |
| Intl.ListFormat | MISSING |  |
| Intl.Segmenter | MISSING |  |
| Intl.Locale | MISSING |  |
| Intl.getCanonicalLocales | MISSING |  |

### Document (21/51 work)

| API | Status | Detail |
|---|---|---|
| document | works |  |
| document.documentElement | works |  |
| document.head | works |  |
| document.body | works |  |
| document.title | works |  |
| document.readyState | works |  |
| document.URL | works |  |
| document.location | MISSING |  |
| document.referrer | works |  |
| document.domain | BROKEN | result check failed (got false) |
| document.cookie | works |  |
| document.characterSet | MISSING |  |
| document.compatMode | MISSING |  |
| document.contentType | MISSING |  |
| document.doctype | MISSING |  |
| document.visibilityState | MISSING |  |
| document.hidden | MISSING |  |
| document.hasFocus | MISSING |  |
| document.activeElement | MISSING |  |
| document.currentScript `*` | works |  |
| document.scripts | MISSING |  |
| document.forms | MISSING |  |
| document.images | MISSING |  |
| document.links | MISSING |  |
| document.getElementById | works |  |
| document.getElementsByClassName | works |  |
| document.getElementsByTagName | works |  |
| document.getElementsByName | MISSING |  |
| document.querySelector | works |  |
| document.querySelectorAll | works |  |
| document.createElement | works |  |
| document.createElementNS | MISSING |  |
| document.createTextNode | works |  |
| document.createComment | works |  |
| document.createDocumentFragment | works |  |
| document.createEvent | MISSING |  |
| document.createRange | MISSING |  |
| document.createTreeWalker | MISSING |  |
| document.createNodeIterator | MISSING |  |
| document.importNode | MISSING |  |
| document.adoptNode | MISSING |  |
| document.implementation.createHTMLDocument | MISSING |  |
| document.elementFromPoint | MISSING |  |
| document.write `*` | works |  |
| document.execCommand | MISSING |  |
| document.fonts | MISSING |  |
| document.styleSheets | MISSING |  |
| document.adoptedStyleSheets | MISSING |  |
| document.startViewTransition `*` | MISSING |  |
| document.fullscreenElement | MISSING |  |
| DOMContentLoaded listener | works |  |

### Node & tree mutation (23/28 work)

| API | Status | Detail |
|---|---|---|
| Node (interface) | works |  |
| appendChild | works |  |
| insertBefore | works |  |
| removeChild | works |  |
| replaceChild | works |  |
| cloneNode(deep) | works |  |
| contains | works |  |
| hasChildNodes | works |  |
| isEqualNode | works |  |
| isSameNode | works |  |
| compareDocumentPosition | works |  |
| normalize | works |  |
| getRootNode | works |  |
| textContent | works |  |
| nodeName/nodeType/nodeValue | works |  |
| parentNode/parentElement | works |  |
| childNodes/firstChild/lastChild | works |  |
| nextSibling/previousSibling | works |  |
| isConnected | works |  |
| ownerDocument | works |  |
| NodeList.prototype.forEach | works |  |
| NodeList iterable (for-of) | works |  |
| HTMLCollection.item/namedItem | works |  |
| Text.splitText | MISSING |  |
| CharacterData.appendData | MISSING |  |
| DocumentFragment (ctor) | BROKEN | threw: TypeError: Illegal constructor |
| Text (ctor) | BROKEN | threw: TypeError: Illegal constructor |
| Comment (ctor) | BROKEN | threw: TypeError: Illegal constructor |

### Element (31/55 work)

| API | Status | Detail |
|---|---|---|
| Element (interface) | works |  |
| HTMLElement (interface) | works |  |
| id/className/tagName | works |  |
| getAttribute/setAttribute | works |  |
| removeAttribute/hasAttribute | works |  |
| toggleAttribute | works |  |
| getAttributeNames | works |  |
| attributes (NamedNodeMap) | MISSING |  |
| classList.add/remove/contains | works |  |
| classList.toggle/replace | works |  |
| dataset | works |  |
| style (inline CSSStyleDeclaration) | works |  |
| style.cssText | works |  |
| innerHTML (get/set) | works |  |
| outerHTML | works |  |
| innerText | works |  |
| insertAdjacentHTML | works |  |
| insertAdjacentElement | works |  |
| insertAdjacentText | works |  |
| append/prepend | works |  |
| before/after | works |  |
| remove | works |  |
| replaceWith | works |  |
| replaceChildren | works |  |
| children/childElementCount | works |  |
| matches | works |  |
| closest | works |  |
| Element.querySelector (scoped) | works |  |
| Element.querySelectorAll (scoped) | works |  |
| querySelector complex selector | BROKEN | result check failed (got false) |
| querySelector attribute selector | BROKEN | result check failed (got false) |
| getElementsByClassName (Element) | works |  |
| getBoundingClientRect | MISSING |  |
| getClientRects | MISSING |  |
| offsetWidth/offsetHeight/offsetTop | MISSING |  |
| offsetParent `*` | MISSING |  |
| clientWidth/clientHeight | MISSING |  |
| scrollWidth/scrollHeight/scrollTop | MISSING |  |
| scrollIntoView | MISSING |  |
| Element.scrollTo | MISSING |  |
| focus/blur | BROKEN | result check failed (got false) |
| click() | works |  |
| hidden property | works |  |
| tabIndex/title/lang/dir | MISSING |  |
| contentEditable/isContentEditable | MISSING |  |
| attachShadow | MISSING |  |
| ShadowRoot (interface) | MISSING |  |
| slot / assignedNodes | MISSING |  |
| animate (Web Animations) | MISSING |  |
| getAnimations | MISSING |  |
| requestFullscreen `*` | MISSING |  |
| setPointerCapture `*` | MISSING |  |
| checkVisibility | MISSING |  |
| HTMLElement.prototype.popover `*` | MISSING |  |
| Element.prototype.computedStyleMap | MISSING |  |

### HTML elements (7/30 work)

| API | Status | Detail |
|---|---|---|
| template.content | MISSING |  |
| HTMLAnchorElement.href (resolved) | MISSING |  |
| HTMLAnchorElement URL parts | MISSING |  |
| HTMLImageElement / new Image() | MISSING |  |
| img.complete/naturalWidth | MISSING |  |
| img.decode `*` | MISSING |  |
| input.value | works |  |
| input.checked | MISSING |  |
| input.setSelectionRange | works |  |
| input.validity/checkValidity | works |  |
| input.files | MISSING |  |
| textarea.value | works |  |
| select.value/options/selectedIndex | MISSING |  |
| new Option() | MISSING |  |
| form.elements | works |  |
| form.submit/requestSubmit | MISSING |  |
| form.reset | works |  |
| button.type/disabled | MISSING |  |
| label.htmlFor/control | MISSING |  |
| canvas.getContext("2d") | MISSING |  |
| canvas.toDataURL | MISSING |  |
| video/audio play()/paused | MISSING |  |
| dialog.showModal/close | MISSING |  |
| details.open | MISSING |  |
| iframe.contentWindow | MISSING |  |
| script element (createElement) | works |  |
| link rel=stylesheet element | MISSING |  |
| style element .sheet | MISSING |  |
| table.insertRow/insertCell | MISSING |  |
| SVGElement / createElementNS svg | MISSING |  |

### Events (21/27 work)

| API | Status | Detail |
|---|---|---|
| EventTarget (ctor) | BROKEN | threw: TypeError: Illegal constructor |
| Event (ctor) | works |  |
| CustomEvent | works |  |
| bubbling + currentTarget | works |  |
| capture phase | works |  |
| stopPropagation | works |  |
| stopImmediatePropagation | works |  |
| preventDefault/defaultPrevented | works |  |
| addEventListener once | works |  |
| addEventListener signal | BROKEN | result check failed (got false) |
| handleEvent object listener | works |  |
| on* handler property | MISSING |  |
| composedPath | MISSING |  |
| MouseEvent | works |  |
| KeyboardEvent | works |  |
| PointerEvent | works |  |
| TouchEvent | works |  |
| FocusEvent | works |  |
| InputEvent | works |  |
| WheelEvent | works |  |
| MessageEvent | works |  |
| ErrorEvent | works |  |
| SubmitEvent | MISSING |  |
| AnimationEvent/TransitionEvent | works |  |
| ClipboardEvent | works |  |
| DragEvent | works |  |
| PromiseRejectionEvent | MISSING |  |

### Observers (3/10 work)

| API | Status | Detail |
|---|---|---|
| MutationObserver (childList) | BROKEN | never settled: callback not called after draining timers |
| MutationObserver (attributes) | BROKEN | never settled: callback not called after draining timers |
| MutationObserver (subtree characterData) | BROKEN | never settled: callback not called after draining timers |
| MutationObserver.takeRecords | BROKEN | result check failed (got false) |
| IntersectionObserver | works |  |
| IntersectionObserver callback fires | BROKEN | never settled: callback not called after draining timers |
| ResizeObserver | works |  |
| ResizeObserver callback fires | BROKEN | never settled: callback not called after draining timers |
| PerformanceObserver | works |  |
| ReportingObserver | MISSING |  |

### Web Components (5/7 work)

| API | Status | Detail |
|---|---|---|
| customElements.define | works |  |
| customElements.get | works |  |
| customElements.whenDefined | works |  |
| attributeChangedCallback | works |  |
| customElements.upgrade | works |  |
| ElementInternals (attachInternals) `*` | MISSING |  |
| CSSStyleSheet constructable | BROKEN | threw: TypeError: Illegal constructor |

### Networking (18/21 work)

| API | Status | Detail |
|---|---|---|
| fetch (returns Promise) | works |  |
| Request | works |  |
| Response | works |  |
| Response.text | works |  |
| Response.json | works |  |
| Response.json (static) | works |  |
| Response.arrayBuffer | works |  |
| Response.blob | works |  |
| Headers | works |  |
| XMLHttpRequest | works |  |
| AbortController | works |  |
| AbortSignal.timeout | works |  |
| AbortSignal.any | works |  |
| WebSocket `*` | MISSING |  |
| EventSource `*` | MISSING |  |
| navigator.sendBeacon | works |  |
| ReadableStream | works |  |
| WritableStream | works |  |
| TransformStream | works |  |
| Response.body (stream) | works |  |
| CompressionStream | MISSING |  |

### URL & encoding (10/11 work)

| API | Status | Detail |
|---|---|---|
| URL | works |  |
| URL setters | works |  |
| URL.canParse | works |  |
| URL.createObjectURL | works |  |
| URL IDNA/punycode host | works |  |
| URLSearchParams | works |  |
| URLSearchParams iteration/sort | works |  |
| TextEncoder | works |  |
| TextDecoder | works |  |
| TextDecoder (non-UTF-8 label) | BROKEN | result check failed (got false) |
| structuredClone | works |  |

### Blob, File & FormData (7/10 work)

| API | Status | Detail |
|---|---|---|
| Blob | works |  |
| Blob.text | works |  |
| Blob.slice | works |  |
| Blob.arrayBuffer | works |  |
| File | works |  |
| FileReader.readAsText | MISSING |  |
| FileReader.readAsDataURL | MISSING |  |
| FormData | works |  |
| FormData(form) | works |  |
| DataTransfer | BROKEN | threw: TypeError: Illegal constructor |

### Storage (3/8 work)

| API | Status | Detail |
|---|---|---|
| localStorage | works |  |
| localStorage.length/key/clear | works |  |
| sessionStorage | works |  |
| Storage property access (localStorage.foo) | BROKEN | result check failed (got false) |
| indexedDB.open | MISSING |  |
| caches (CacheStorage) `*` | MISSING |  |
| navigator.storage.estimate `*` | MISSING |  |
| cookieStore `*` | MISSING |  |

### Location, History & Navigator (12/25 work)

| API | Status | Detail |
|---|---|---|
| location.href | works |  |
| location parts | works |  |
| location.assign/replace/reload `*` | works |  |
| history.length/state | MISSING |  |
| history.pushState | BROKEN | threw: TypeError: cannot convert 'null' or 'undefined' to object |
| history.replaceState | BROKEN | threw: TypeError: cannot convert 'null' or 'undefined' to object |
| history.back/forward/go `*` | works |  |
| navigation (Navigation API) `*` | MISSING |  |
| navigator.userAgent | works |  |
| navigator.language(s) | works |  |
| navigator.platform/vendor | works |  |
| navigator.onLine | works |  |
| navigator.cookieEnabled | works |  |
| navigator.hardwareConcurrency | works |  |
| navigator.maxTouchPoints | works |  |
| navigator.webdriver | works |  |
| navigator.userAgentData | MISSING |  |
| navigator.clipboard `*` | MISSING |  |
| navigator.serviceWorker `*` | MISSING |  |
| navigator.permissions.query `*` | MISSING |  |
| navigator.mediaDevices `*` | MISSING |  |
| navigator.geolocation `*` | MISSING |  |
| navigator.share `*` | MISSING |  |
| navigator.vibrate `*` | MISSING |  |
| navigator.connection | MISSING |  |

### CSSOM & view (4/12 work)

| API | Status | Detail |
|---|---|---|
| getComputedStyle | BROKEN | threw: TypeError: not a callable function |
| getComputedStyle reflects inline style | BROKEN | result check failed (got false) |
| matchMedia | works |  |
| matchMedia evaluates width | BROKEN | result check failed (got false) |
| MediaQueryList.addEventListener | works |  |
| CSS.supports | MISSING |  |
| CSS.escape | MISSING |  |
| document.styleSheets[i].cssRules | MISSING |  |
| CSSStyleSheet.insertRule | MISSING | instance unavailable: cannot convert 'null' or 'undefined' to object |
| DOMRect | works |  |
| DOMMatrix | MISSING |  |
| DOMPoint | works |  |

### Parsing & serialization (0/6 work)

| API | Status | Detail |
|---|---|---|
| DOMParser text/html | MISSING |  |
| DOMParser image/svg+xml | MISSING |  |
| DOMParser application/xml | MISSING |  |
| XMLSerializer | MISSING |  |
| Range.createContextualFragment | MISSING | instance unavailable: not a callable function |
| Element.setHTMLUnsafe | MISSING |  |

### Range & Selection (0/10 work)

| API | Status | Detail |
|---|---|---|
| Range (ctor) | BROKEN | threw: TypeError: Illegal constructor |
| Range.selectNodeContents/toString | MISSING | instance unavailable: not a callable function |
| Range.setStart/setEnd | MISSING | instance unavailable: not a callable function |
| Range.deleteContents | MISSING | instance unavailable: not a callable function |
| Range.extractContents/cloneContents | MISSING | instance unavailable: not a callable function |
| Range.insertNode/surroundContents | MISSING | instance unavailable: not a callable function |
| Range.getBoundingClientRect | MISSING | instance unavailable: not a callable function |
| StaticRange | MISSING |  |
| Selection.addRange/toString | BROKEN | threw: TypeError: not a callable function |
| Selection.collapse/extend | MISSING |  |

### Crypto & performance (6/9 work)

| API | Status | Detail |
|---|---|---|
| crypto.getRandomValues | MISSING |  |
| crypto.randomUUID | MISSING |  |
| crypto.subtle.digest | MISSING |  |
| performance.now | works |  |
| performance.timeOrigin | works |  |
| performance.mark/measure | works |  |
| performance.getEntriesByType | works |  |
| performance.timing (legacy) | works |  |
| Date.now | works |  |

### Console (2/6 work)

| API | Status | Detail |
|---|---|---|
| console.log | works |  |
| console.error/warn/info/debug | works |  |
| console.table/dir | MISSING |  |
| console.group/groupEnd | MISSING |  |
| console.time/timeEnd | MISSING |  |
| console.assert/count/trace | MISSING |  |

### Workers & messaging (0/4 work)

| API | Status | Detail |
|---|---|---|
| Worker `*` | MISSING |  |
| SharedWorker `*` | MISSING |  |
| BroadcastChannel | MISSING |  |
| window message event via postMessage | MISSING |  |

### Graphics & media (0/12 work)

| API | Status | Detail |
|---|---|---|
| CanvasRenderingContext2D | MISSING |  |
| OffscreenCanvas | MISSING |  |
| ImageData | MISSING |  |
| Path2D | MISSING |  |
| createImageBitmap `*` | MISSING |  |
| WebGLRenderingContext `*` | MISSING |  |
| canvas.getContext("webgl") | MISSING |  |
| AudioContext `*` | MISSING |  |
| MediaSource `*` | MISSING |  |
| Notification | MISSING |  |
| speechSynthesis `*` | MISSING |  |
| FontFace | MISSING |  |

## Ranked top 25: missing or broken APIs, ordered by how much real sites depend on them

**This ranking is my judgement.** It is based on how often each API shows up in
widely used frameworks, libraries and site scripts. It was not computed from
usage data. Each entry cites the census rows behind it. Selector rows are
excluded (see caveat 3).

| # | API (census status) | Why sites depend on it |
|---:|---|---|
| 1 | `Element.getBoundingClientRect` (MISSING) | Used for lazy loading, sticky headers, tooltips and popovers, virtual lists and analytics viewability. Calling it throws a `TypeError` on very many pages. |
| 2 | `getComputedStyle` (BROKEN: the result has no `getPropertyValue`, and inline `display:none` is not reflected) | Used by jQuery `.css()`, animation libraries, UI kits and feature probes. |
| 3 | Layout metrics `offsetWidth/Height/Top`, `offsetParent`, `clientWidth/Height`, `scrollWidth/Height/Top` (MISSING) | Used by carousels, responsive JS, scroll handlers, jQuery dimensions and infinite scroll. |
| 4 | `Intl.*` (all 10 rows MISSING); `Date.prototype.toLocaleDateString` (BROKEN: "Function Unimplemented"); `Number.prototype.toLocaleString` (BROKEN: no grouping) | Used for date, price and number formatting on almost every commerce, news and SaaS page, and by i18n libraries such as react-intl, date-fns/luxon and formatjs. |
| 5 | `Element.attributes` (MISSING) | Template engines and frameworks (Vue, Alpine, htmx, Angular) iterate over attributes. Sanitizers do the same. |
| 6 | `history.pushState/replaceState` (BROKEN: no-op stubs) and `history.state` (MISSING) | Every SPA router (Next.js, React Router, Vue Router, SvelteKit) uses these. |
| 7 | `MutationObserver` (BROKEN: the callback never fires and `takeRecords` is always empty) | Used by frameworks, consent and ad scripts, polyfills, and React/Vue devtools hooks. Many widgets wait for mutations. |
| 8 | `IntersectionObserver` (BROKEN: the callback never fires) | Used for lazy-loaded images and content, infinite scroll and impression tracking. Content that waits on it never appears. |
| 9 | `crypto.getRandomValues` / `crypto.randomUUID` (MISSING) | Used by the `uuid`/`nanoid` packages, auth SDKs (PKCE, nonces), analytics IDs and Firebase. |
| 10 | `new Image()` (MISSING) | Used for analytics and tracking pixels, image preloading and carousels. |
| 11 | `document.activeElement` (MISSING), which also breaks focus checks (`focus/blur` row BROKEN) | Used by accessibility code, modals and focus traps, form libraries and keyboard navigation. |
| 12 | `HTMLAnchorElement.href` and URL parts (`pathname`, `search`, `hostname`) (MISSING) | Used by client-side routers that intercept links, analytics outbound-link tracking and URL parsing done with an `<a>` element. |
| 13 | `document.createElementNS` (MISSING) | Used for SVG in React, Vue and Preact, by D3 and charting libraries, and by icon systems. |
| 14 | CSSOM: `document.styleSheets` (MISSING), `CSSStyleSheet` constructor (BROKEN: Illegal constructor), `<style>.sheet` (MISSING), `insertRule` | CSS-in-JS (emotion and styled-components in production use `insertRule`), constructable stylesheets for web components, and theme switchers. |
| 15 | `window.postMessage` (MISSING), `MessageChannel` (MISSING) | Used by OAuth popups, embeds, payment iframes and cross-frame SDKs. React's scheduler prefers `MessageChannel` and falls back to `setTimeout` without it. |
| 16 | `document.createEvent` (MISSING) | Legacy, but still used by jQuery-era code, polyfills and analytics to synthesize events. |
| 17 | `<template>.content` (MISSING) | Used by Lit, Stencil, Vue and other web-component frameworks, and by htmx and Turbo. |
| 18 | Form-control IDL attributes: `select.options/selectedIndex`, `button.type`, `label.htmlFor`, `input.checked`, `input.files`, `form.requestSubmit`, `new Option()` (all MISSING) | Used by form libraries, validation, checkout flows and search UIs. |
| 19 | `document.visibilityState` / `document.hidden` (MISSING) | Used by analytics, video players, polling throttles and SPA frameworks. |
| 20 | `Element.attachShadow` / Shadow DOM (MISSING) | Used by web components on large sites (YouTube, GitHub, Salesforce, design systems). |
| 21 | `new EventTarget()` (BROKEN: Illegal constructor) | Modern libraries subclass `EventTarget` as their event emitter. |
| 22 | `ResizeObserver` (BROKEN: the callback never fires) | Used by charts, virtualized lists, responsive components and editors. |
| 23 | `DOMParser` (MISSING) | Used by sanitizers, SVG inliners, htmx/Turbo and HTML-fragment parsing. |
| 24 | `document.createRange` / `Range` (MISSING; the constructor is "Illegal constructor") and the `Selection.addRange` path (BROKEN) | Used by rich-text editors, copy-to-clipboard helpers and `createContextualFragment` HTML insertion. |
| 25 | Canvas 2D: `canvas.getContext` (MISSING), `CanvasRenderingContext2D`, `ImageData`, `Path2D` (MISSING) | Used by Chart.js-style charts, image editing, captchas and fingerprinting checks. |

**Next tier (not ranked):** these rows are also measured as missing or broken.
- Events and observers: `addEventListener({signal})` is not honoured, and
  `composedPath` is missing.
- Console: `console.group`, `console.table`, `console.time` and
  `console.assert` are missing, so calling them throws.
- CSS: `CSS.supports` and `CSS.escape` are missing, and `matchMedia` evaluation
  is BROKEN (`(min-width: 1px)` is false at 1280px).
- Storage and network: `indexedDB`, `WebSocket`, `EventSource` and `FileReader`
  are missing. Property-style `localStorage.key` access is BROKEN.
- Document: `document.location`, `document.forms/images/links/scripts` and
  `getElementsByName` are missing. `document.domain` is BROKEN (an empty string).
- Element: `scrollIntoView` and `Element.scrollTo` are missing.
- Constructors: `Text`, `Comment` and `DocumentFragment` throw "Illegal constructor".
- Encoding: `TextDecoder('windows-1252')` decodes to an empty string.
- Platform APIs: `Worker`, `BroadcastChannel`, `navigator.clipboard` and
  `navigator.serviceWorker` are missing.

## What was not measured

- **The engine's selector matcher.** Selector APIs here use the bindings' fallback matcher (see caveat 3).
- **Real network behaviour** of `fetch`/XHR/`sendBeacon`, including CORS and
  responses. The `fetch(data:)` row was dropped because the result depends on the
  engine's network delivery, which this harness does not have.
- **Layout-dependent values.** There is no layout in this harness, so even
  present geometry APIs could not be checked for correct numbers.
- **Event delivery driven by the engine:** real user input, `load` and `DOMContentLoaded`
  ordering during an actual navigation, and the frame-driven `requestAnimationFrame` cadence.
  Here, rAF runs on the bindings' virtual timer clock.
- **Anything specific to macOS, WebKit-fallback, or the chrome/shelf WebViews.**
- **Presence-only rows (`*`):** whether the call actually works.
- **Spec conformance beyond one smoke call** (partly sampled in the WPT section below).

## Feasibility: running upstream WPT testharness.js tests in this harness

**Time-boxed at about 30 minutes. Nothing was vendored.** I downloaded single
files from `web-platform-tests/wpt@master` (`resources/testharness.js`, 13
`dom/nodes` and `url` test files, and the helper and data files they load) into
a scratch directory outside the repo. A throwaway Rust test, which was not
committed, then ran them with the same `DomBindings` setup as the census. That
test:
1. parses the test page and evaluates its `<script>` elements in order, loading
   `src=` helpers from the local files and skipping `testharnessreport.js`;
2. calls `setup({output: false})` and `add_completion_callback(...)` to
   collect the results;
3. fires `DOMContentLoaded` and `load`, then alternates `run_timers` with
   answering the net bridge's queued requests from local files.

### Results (one run, wpt master on 2026-10-03, subtests passed / subtests reported)

| Test | Harness status | Pass / total |
|---|---|---|
| dom/nodes/Node-appendChild.html | OK | 5 / 11 |
| dom/nodes/Node-insertBefore.html | OK | 6 / 40 |
| dom/nodes/Node-removeChild.html | OK | 1 / 28 |
| dom/nodes/Node-cloneNode.html | OK | 3 / 135 |
| dom/nodes/Element-closest.html | OK | 10 / 29 (fallback matcher, see caveat 3) |
| dom/nodes/Element-classlist.html | OK (one setup script threw) | 179 / 284 |
| dom/nodes/ParentNode-append.html | OK | 16 / 25 |
| dom/nodes/Document-createElement.html | OK | 0 / 147 |
| dom/nodes/Node-textContent.html | ERROR in setup (`cannot convert 'null' or 'undefined' to object`) | 0 / 0 |
| dom/nodes/Element-matches.html | TIMEOUT (needs iframes) | 0 / 1 |
| url/urlsearchparams-append, -get, -set, -sort, url-searchparams (.any.js) | OK | 29 / 29 |
| url/urlsearchparams-delete.any.js | OK | 6 / 8 |
| url/url-constructor.any.js, url/url-origin.any.js | OK, but the data load fails | 0 / 1 each ("Loading data…") |
| url/url-setters.any.js | OK, but the data load fails | 0 / 1 (`subsetTestByKey` not defined) |

**Conclusion: this is feasible.** `testharness.js` itself loads and runs to
completion in Boa with the bindings, and real pass/fail numbers come back for
most of the sampled tests. To run the `dom/nodes` and `url` subsets properly,
the following are missing:

1. **A runner.** It needs to:
   - resolve `<script src>` against a WPT checkout given by an environment
     variable, so nothing is vendored;
   - handle `// META: script=` and `// META: global=` in `.any.js` files.
     url-setters needs `/common/subset-tests-by-key.js` this way;
   - serve WPT resource files to `fetch` through the net bridge;
   - store expected results so that regressions show up as diffs.

   All of this can sit in a test target of rustkit-bindings or in a small tools
   crate on Linux. It needs no engine changes.
2. **`testharness.js` output must be disabled** (`setup({output:false})`).
   With output on, the harness never completes: a `TypeError: not a callable
   function` was reported during `load`. I did not root-cause it. The likely
   causes are DOM APIs that the census lists as missing, such as
   `createElementNS`.
3. **Boa's `JSON.parse` rejects lone surrogate escapes** (`\uD800`). Every
   data-driven `url/` test loads `urltestdata.json` (url-constructor,
   url-origin and others), so they all fail at the data-loading step before any
   URL assertion runs. This is a JS-engine gap, not a bindings gap. A runner could
   work around it only by pre-processing the data, which would weaken the test.
4. **DOM gaps the census already lists** are where most `dom/nodes` failures
   come from. For example: `createElementNS`, XML documents
   (`document.implementation`), `Text`/`Comment` constructors, `attributes`, and
   `doctype`. Document-createElement (0/147) and Node-cloneNode (3/135) fail
   mostly on these.
5. **Tests that use iframes or multiple documents** (Element-matches, and many
   `dom/nodes` tests that use `<iframe>` or `createHTMLDocument`) need a
   browsing-context model that this harness does not have. Those will time out
   until that model exists.
6. **The engine's selector matcher** would have to be exposed for the selector
   tests (`Element-closest`, `Element-matches`, `ParentNode-querySelector-All`)
   to mean anything here.

**Not measured:** any other WPT directory; how stable these numbers are across
WPT revisions; and the run time of a full `dom/nodes` pass (about 30 s for the
13 sampled files in a debug build).
