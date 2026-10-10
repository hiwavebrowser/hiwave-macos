# Web API census: 2026-10-05

A rerun of the [2026-10-03 census](WEB_API_CENSUS_2026-10-03.md) at a later
`develop`. It measures the JavaScript/DOM surface that a page script sees in
RustKit's bindings (`rustkit-bindings` on top of `rustkit-js`/Boa). This is
measurement only. No engine code changed. The only code change is 84 added
probe rows in the census probe, which is a test file.

- **develop SHA measured:** `15d2c3a6013e1e70593fa30656670c203a627fd2`
  (merge of #527). The previous census measured `91fa24e8`. 159 commits
  separate the two, and about 40 of them are script-API pull requests.
- **Where it ran:** a Linux cloud session. No macOS gates were run, and no GPU
  or engine crates were run either.
- **Probe:** `crates/rustkit-bindings/tests/census/web_api_census_probe.js`, driven by
  `crates/rustkit-bindings/tests/web_api_census.rs`. The harness is unchanged.
  The probe has 84 new rows at the end, under `Added 2026-10-05`.
- **Raw data:** `docs/census/web_api_census_2026-10-05.json`, one record per probed
  API. The 2026-10-03 data stays in `docs/census/web_api_census.json` so the
  two runs can be compared.

## How to regenerate

```bash
git checkout 15d2c3a6013e1e70593fa30656670c203a627fd2   # or any later develop
WEB_API_CENSUS_OUT=$PWD/docs/census/web_api_census_2026-10-05.json \
  cargo test -p rustkit-bindings --test web_api_census -- --nocapture
python3 scripts/census/web_api_census_tables.py docs/census/web_api_census_2026-10-05.json
```

The test prints `census: total=… works=… broken=… missing=…` and every row that
does not work. The Python script produces the Totals and Per-area sections below.
This run printed `census: total=528 works=408 broken=26 missing=94`.

Two runs went into this document:
1. **The original 444-row probe**, unchanged, at the new SHA. It printed
   `total=444 works=333 broken=21 missing=90`. The "Changed since" section
   below compares this run with 2026-10-03, row for row.
2. **The extended 528-row probe**, which is the 444 rows plus 84 new ones. The
   Totals, Per-area and JSON sections come from this run. The original 444 rows
   gave identical statuses in both runs.

## Method

The method is the same as on 2026-10-03, and so are the classification
(MISSING, BROKEN, works) and the caveats. In short:

- **MISSING:** the API's path resolves to `undefined`. An `@tag.member` path
  is resolved on a fresh instance.
- **BROKEN:** the API is present, but its one smoke call threw, returned the
  wrong result, or never settled after the timers were drained.
- **works:** the API is present, and one small smoke call returned the checked
  result.

**Caveats. Read these before quoting numbers.**
1. A single smoke call shows that basic use works. It does not show spec
   conformance.
2. 36 rows, marked `*`, can only check that a member is callable. 8 of those
   36 count as works.
3. **Selector matching is not the engine's.** The harness uses the bindings'
   fallback matcher. The two "complex/attribute selector" rows are BROKEN *in
   this harness only*, and they are left out of the ranking.
4. There is no network. `fetch` and XHR are measured only up to queueing a
   request, and through `Request`/`Response`/`Headers`.
5. **There is no layout in this harness.** The geometry rows that now "work"
   (`getBoundingClientRect`, the `offset*`/`client*`/`scroll*` metrics) check
   only that the call returns numbers. The numbers themselves are not
   checked. For the same reason, the **`ResizeObserver callback fires` row is
   BROKEN in this harness only**. The observed element has no laid-out size,
   so it stays at 0×0. The new observer (#507) reports only the sizes that the
   layout publishes, and the initial 0×0 size is not a change to report.
   (Measured: no callback after draining timers, with `set_dimensions` set and
   no layout.) This row is left out of the ranking. It needs a run where the
   engine lays out the page.
6. Some areas have several rows, so the counts are probe rows, not distinct
   interfaces.

## Totals

- APIs probed: **528**
- works: **408** (77.3%)
- broken: **26** (4.9%)
- missing: **94** (17.8%)
- of which presence-only smoke checks (marked `*`): 36 (8 counted as works)

| Area | Probed | Works | Broken | Missing |
|---|---:|---:|---:|---:|
| Window & globals | 37 | 31 | 1 | 5 |
| Timers & scheduling | 11 | 9 | 0 | 2 |
| JavaScript builtins | 52 | 51 | 0 | 1 |
| Intl | 20 | 16 | 0 | 4 |
| Document | 57 | 48 | 1 | 8 |
| Node & tree mutation | 28 | 23 | 3 | 2 |
| Element | 59 | 48 | 2 | 9 |
| HTML elements | 38 | 27 | 1 | 10 |
| Events | 32 | 29 | 2 | 1 |
| Observers | 13 | 7 | 5 | 1 |
| Web Components | 10 | 8 | 1 | 1 |
| Networking | 31 | 26 | 2 | 3 |
| URL & encoding | 17 | 16 | 1 | 0 |
| Blob, File & FormData | 14 | 11 | 1 | 2 |
| Storage | 8 | 3 | 1 | 4 |
| Location, History & Navigator | 30 | 20 | 0 | 10 |
| CSSOM & view | 21 | 20 | 1 | 0 |
| Parsing & serialization | 6 | 1 | 2 | 3 |
| Range & Selection | 10 | 0 | 2 | 8 |
| Crypto & performance | 12 | 11 | 0 | 1 |
| Console | 6 | 2 | 0 | 4 |
| Workers & messaging | 4 | 0 | 0 | 4 |
| Graphics & media | 12 | 1 | 0 | 11 |
| **All** | **528** | **408** | **26** | **94** |

## Per-area results

### Window & globals (31/37 work)

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
| window.outerWidth/outerHeight | works |  |
| window.opener/closed | works |  |
| screen.availWidth/colorDepth | works |  |
| screen.orientation | works |  |
| window.scroll (alias) | works |  |
| escape/unescape | works |  |
| DOMException | BROKEN | result check failed (got false) |
| Window/Navigator/Location/History/Screen interfaces | works |  |

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

### JavaScript builtins (51/52 work)

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
| Date.prototype.toLocaleDateString | works |  |
| Number.prototype.toLocaleString | works |  |
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

### Intl (16/20 work)

| API | Status | Detail |
|---|---|---|
| Intl | works |  |
| Intl.DateTimeFormat | works |  |
| Intl.NumberFormat | works |  |
| Intl.Collator | works |  |
| Intl.PluralRules | works |  |
| Intl.RelativeTimeFormat | works |  |
| Intl.ListFormat | works |  |
| Intl.Segmenter | MISSING |  |
| Intl.Locale | MISSING |  |
| Intl.getCanonicalLocales | works |  |
| Intl.NumberFormat currency | works |  |
| Intl.NumberFormat compact | works |  |
| Intl.NumberFormat.formatToParts | works |  |
| Intl.DateTimeFormat options | works |  |
| Intl.DateTimeFormat.formatToParts | works |  |
| Intl.DateTimeFormat resolvedOptions().timeZone | works |  |
| Intl.DisplayNames | MISSING |  |
| Intl.supportedValuesOf | MISSING |  |
| Date.prototype.toLocaleTimeString | works |  |
| Date.prototype.toLocaleString (en-US, UTC) | works |  |

### Document (48/57 work)

| API | Status | Detail |
|---|---|---|
| document | works |  |
| document.documentElement | works |  |
| document.head | works |  |
| document.body | works |  |
| document.title | works |  |
| document.readyState | works |  |
| document.URL | works |  |
| document.location | works |  |
| document.referrer | works |  |
| document.domain | BROKEN | result check failed (got false) |
| document.cookie | works |  |
| document.characterSet | works |  |
| document.compatMode | works |  |
| document.contentType | works |  |
| document.doctype | works |  |
| document.visibilityState | works |  |
| document.hidden | works |  |
| document.hasFocus | works |  |
| document.activeElement | works |  |
| document.currentScript `*` | works |  |
| document.scripts | works |  |
| document.forms | works |  |
| document.images | works |  |
| document.links | works |  |
| document.getElementById | works |  |
| document.getElementsByClassName | works |  |
| document.getElementsByTagName | works |  |
| document.getElementsByName | works |  |
| document.querySelector | works |  |
| document.querySelectorAll | works |  |
| document.createElement | works |  |
| document.createElementNS | works |  |
| document.createTextNode | works |  |
| document.createComment | works |  |
| document.createDocumentFragment | works |  |
| document.createEvent | works |  |
| document.createRange | MISSING |  |
| document.createTreeWalker | MISSING |  |
| document.createNodeIterator | MISSING |  |
| document.importNode | works |  |
| document.adoptNode | works |  |
| document.implementation.createHTMLDocument | MISSING |  |
| document.elementFromPoint | MISSING |  |
| document.write `*` | works |  |
| document.execCommand | MISSING |  |
| document.fonts | works |  |
| document.styleSheets | works |  |
| document.adoptedStyleSheets | works |  |
| document.startViewTransition `*` | MISSING |  |
| document.fullscreenElement | works |  |
| DOMContentLoaded listener | works |  |
| document.anchors/embeds | works |  |
| document.fonts.ready resolves | works |  |
| document.fonts.check/load | works |  |
| document.fullscreenEnabled | works |  |
| document.scrollingElement | works |  |
| document.implementation (DOMImplementation) | MISSING |  |

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

### Element (48/59 work)

| API | Status | Detail |
|---|---|---|
| Element (interface) | works |  |
| HTMLElement (interface) | works |  |
| id/className/tagName | works |  |
| getAttribute/setAttribute | works |  |
| removeAttribute/hasAttribute | works |  |
| toggleAttribute | works |  |
| getAttributeNames | works |  |
| attributes (NamedNodeMap) | works |  |
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
| getBoundingClientRect | works |  |
| getClientRects | works |  |
| offsetWidth/offsetHeight/offsetTop | works |  |
| offsetParent `*` | works |  |
| clientWidth/clientHeight | works |  |
| scrollWidth/scrollHeight/scrollTop | works |  |
| scrollIntoView | works |  |
| Element.scrollTo | works |  |
| focus/blur | works |  |
| click() | works |  |
| hidden property | works |  |
| tabIndex/title/lang/dir | MISSING |  |
| contentEditable/isContentEditable | MISSING |  |
| attachShadow | works |  |
| ShadowRoot (interface) | works |  |
| slot / assignedNodes | works |  |
| animate (Web Animations) | MISSING |  |
| getAnimations | MISSING |  |
| requestFullscreen `*` | MISSING |  |
| setPointerCapture `*` | MISSING |  |
| checkVisibility | MISSING |  |
| HTMLElement.prototype.popover `*` | MISSING |  |
| Element.prototype.computedStyleMap | MISSING |  |
| Element.scrollLeft/scrollBy | works |  |
| slot.assignedElements / assignedSlot | works |  |
| closed shadow root hides shadowRoot | works |  |
| event retargeting across shadow boundary | works |  |

### HTML elements (27/38 work)

| API | Status | Detail |
|---|---|---|
| template.content | works |  |
| HTMLAnchorElement.href (resolved) | works |  |
| HTMLAnchorElement URL parts | works |  |
| HTMLImageElement / new Image() | works |  |
| img.complete/naturalWidth | MISSING |  |
| img.decode `*` | MISSING |  |
| input.value | works |  |
| input.checked | works |  |
| input.setSelectionRange | works |  |
| input.validity/checkValidity | works |  |
| input.files | works |  |
| textarea.value | works |  |
| select.value/options/selectedIndex | works |  |
| new Option() | works |  |
| form.elements | works |  |
| form.submit/requestSubmit | works |  |
| form.reset | works |  |
| button.type/disabled | works |  |
| label.htmlFor/control | works |  |
| canvas.getContext("2d") | MISSING |  |
| canvas.toDataURL | MISSING |  |
| video/audio play()/paused | MISSING |  |
| dialog.showModal/close | MISSING |  |
| details.open | works |  |
| iframe.contentWindow | MISSING |  |
| script element (createElement) | works |  |
| link rel=stylesheet element | MISSING |  |
| style element .sheet | works |  |
| table.insertRow/insertCell | MISSING |  |
| SVGElement / createElementNS svg | works |  |
| option.selected/defaultSelected | works |  |
| input.defaultChecked/indeterminate | MISSING |  |
| setCustomValidity/reportValidity | works |  |
| fieldset.elements/disabled | BROKEN | result check failed (got false) |
| form.length/namedItem | works |  |
| SubmitEvent.submitter | works |  |
| HTMLAnchorElement.origin | works |  |
| HTMLDetailsElement / details toggle | works |  |

### Events (29/32 work)

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
| composedPath | works |  |
| MouseEvent | works |  |
| KeyboardEvent | works |  |
| PointerEvent | works |  |
| TouchEvent | works |  |
| FocusEvent | works |  |
| InputEvent | works |  |
| WheelEvent | works |  |
| MessageEvent | works |  |
| ErrorEvent | works |  |
| SubmitEvent | works |  |
| AnimationEvent/TransitionEvent | works |  |
| ClipboardEvent | works |  |
| DragEvent | works |  |
| PromiseRejectionEvent | works |  |
| UIEvent / CompositionEvent | works |  |
| HashChangeEvent / PopStateEvent / PageTransitionEvent | works |  |
| StorageEvent / ProgressEvent | works |  |
| KeyboardEvent.getModifierState | works |  |
| MediaQueryListEvent | works |  |

### Observers (7/13 work)

| API | Status | Detail |
|---|---|---|
| MutationObserver (childList) | BROKEN | never settled: callback not called after draining timers |
| MutationObserver (attributes) | BROKEN | never settled: callback not called after draining timers |
| MutationObserver (subtree characterData) | BROKEN | never settled: callback not called after draining timers |
| MutationObserver.takeRecords | BROKEN | result check failed (got false) |
| IntersectionObserver | works |  |
| IntersectionObserver callback fires | works |  |
| ResizeObserver | works |  |
| ResizeObserver callback fires | BROKEN | never settled: callback not called after draining timers |
| PerformanceObserver | works |  |
| ReportingObserver | MISSING |  |
| IntersectionObserver rootMargin/thresholds | works |  |
| IntersectionObserver.takeRecords | works |  |
| IntersectionObserverEntry / ResizeObserverEntry / MutationRecord | works |  |

### Web Components (8/10 work)

| API | Status | Detail |
|---|---|---|
| customElements.define | works |  |
| customElements.get | works |  |
| customElements.whenDefined | works |  |
| attributeChangedCallback | works |  |
| customElements.upgrade | works |  |
| ElementInternals (attachInternals) `*` | MISSING |  |
| CSSStyleSheet constructable | works |  |
| customElements.getName | works |  |
| connected/disconnectedCallback | BROKEN | result check failed (got false) |
| CustomElementRegistry (interface) | works |  |

### Networking (26/31 work)

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
| XMLHttpRequest.upload / getAllResponseHeaders | works |  |
| XMLHttpRequest synchronous open | BROKEN | threw: NotSupportedError: Failed to execute 'open' on 'XMLHttpRequest': synchronous requests are not supported |
| Request.clone / Response.clone | works |  |
| Response.error / Response.redirect | works |  |
| Response.formData (urlencoded) | works |  |
| ReadableStream.tee / pipeThrough | works |  |
| ReadableStream async iteration | works |  |
| CountQueuingStrategy / ByteLengthQueuingStrategy | works |  |
| TextEncoderStream / TextDecoderStream | works |  |
| byob reader | BROKEN | threw: NotSupportedError: Failed to execute 'getReader' on 'ReadableStream': BYOB readers are not supported. |

### URL & encoding (16/17 work)

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
| URL.revokeObjectURL | works |  |
| URL.parse | works |  |
| webkitURL alias | works |  |
| URLSearchParams.size | works |  |
| TextEncoder.encodeInto | works |  |
| TextDecoder fatal | works |  |

### Blob, File & FormData (11/14 work)

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
| Blob.bytes | works |  |
| File.lastModified/name | works |  |
| FormData.entries/getAll | works |  |
| AbortSignal.abort / throwIfAborted | works |  |

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

### Location, History & Navigator (20/30 work)

| API | Status | Detail |
|---|---|---|
| location.href | works |  |
| location parts | works |  |
| location.assign/replace/reload `*` | works |  |
| history.length/state | works |  |
| history.pushState | works |  |
| history.replaceState | works |  |
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
| history.scrollRestoration | works |  |
| popstate on history.back() | works |  |
| pushState cross-origin throws SecurityError | works |  |
| navigator.plugins/mimeTypes/pdfViewerEnabled | works |  |
| navigator.doNotTrack/javaEnabled | works |  |

### CSSOM & view (20/21 work)

| API | Status | Detail |
|---|---|---|
| getComputedStyle | works |  |
| getComputedStyle reflects inline style | works |  |
| matchMedia | works |  |
| matchMedia evaluates width | BROKEN | result check failed (got false) |
| MediaQueryList.addEventListener | works |  |
| CSS.supports | works |  |
| CSS.escape | works |  |
| document.styleSheets[i].cssRules | works |  |
| CSSStyleSheet.insertRule | works |  |
| DOMRect | works |  |
| DOMMatrix | works |  |
| DOMPoint | works |  |
| CSSStyleSheet.replaceSync + adoptedStyleSheets | works |  |
| CSSStyleSheet.replace (Promise) | works |  |
| CSSStyleSheet.deleteRule | works |  |
| CSSStyleRule.selectorText / style.setProperty | works |  |
| <style>.sheet reflects text | works |  |
| DOMMatrix from CSS string | works |  |
| DOMMatrix.multiply/inverse | works |  |
| DOMMatrixReadOnly / WebKitCSSMatrix | works |  |
| DOMRectReadOnly / DOMPointReadOnly | works |  |

### Parsing & serialization (1/6 work)

| API | Status | Detail |
|---|---|---|
| DOMParser text/html | works |  |
| DOMParser image/svg+xml | BROKEN | threw: NotSupportedError: Failed to execute 'parseFromString' on 'DOMParser': XML documents are not supported yet. |
| DOMParser application/xml | BROKEN | threw: NotSupportedError: Failed to execute 'parseFromString' on 'DOMParser': XML documents are not supported yet. |
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

### Crypto & performance (11/12 work)

| API | Status | Detail |
|---|---|---|
| crypto.getRandomValues | works |  |
| crypto.randomUUID | works |  |
| crypto.subtle.digest | MISSING |  |
| performance.now | works |  |
| performance.timeOrigin | works |  |
| performance.mark/measure | works |  |
| performance.getEntriesByType | works |  |
| performance.timing (legacy) | works |  |
| Date.now | works |  |
| crypto.getRandomValues rejects Float32Array | works |  |
| performance.getEntriesByName / clearMarks | works |  |
| performance.toJSON | works |  |

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

### Graphics & media (1/12 work)

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
| FontFace | works |  |


## Changed since 2026-10-03

This section compares the 444 rows the two runs share (same area and name) by
status. **80 rows changed status. None got worse.**

| | Rows |
|---|---:|
| MISSING → works | 69 |
| BROKEN → works | 9 |
| MISSING → BROKEN (now present, smoke call fails) | 2 |
| works → BROKEN or MISSING, or BROKEN → MISSING | **0** |

On the shared 444 rows: works went from 255 to **333**, broken from 28 to **21**,
and missing from 161 to **90**.

### MISSING → works (69)

| Area | API | 2026-10-05 detail |
|---|---|---|
| Intl | Intl |  |
| Intl | Intl.DateTimeFormat |  |
| Intl | Intl.NumberFormat |  |
| Intl | Intl.Collator |  |
| Intl | Intl.PluralRules |  |
| Intl | Intl.RelativeTimeFormat |  |
| Intl | Intl.ListFormat |  |
| Intl | Intl.getCanonicalLocales |  |
| Document | document.location |  |
| Document | document.characterSet |  |
| Document | document.compatMode |  |
| Document | document.contentType |  |
| Document | document.doctype |  |
| Document | document.visibilityState |  |
| Document | document.hidden |  |
| Document | document.hasFocus |  |
| Document | document.activeElement |  |
| Document | document.scripts |  |
| Document | document.forms |  |
| Document | document.images |  |
| Document | document.links |  |
| Document | document.getElementsByName |  |
| Document | document.createElementNS |  |
| Document | document.createEvent |  |
| Document | document.importNode |  |
| Document | document.adoptNode |  |
| Document | document.fonts |  |
| Document | document.styleSheets |  |
| Document | document.adoptedStyleSheets |  |
| Document | document.fullscreenElement |  |
| Element | attributes (NamedNodeMap) |  |
| Element | getBoundingClientRect |  |
| Element | getClientRects |  |
| Element | offsetWidth/offsetHeight/offsetTop |  |
| Element | offsetParent `*` |  |
| Element | clientWidth/clientHeight |  |
| Element | scrollWidth/scrollHeight/scrollTop |  |
| Element | scrollIntoView |  |
| Element | Element.scrollTo |  |
| Element | attachShadow |  |
| Element | ShadowRoot (interface) |  |
| Element | slot / assignedNodes |  |
| HTML elements | template.content |  |
| HTML elements | HTMLAnchorElement.href (resolved) |  |
| HTML elements | HTMLAnchorElement URL parts |  |
| HTML elements | HTMLImageElement / new Image() |  |
| HTML elements | input.checked |  |
| HTML elements | input.files |  |
| HTML elements | select.value/options/selectedIndex |  |
| HTML elements | new Option() |  |
| HTML elements | form.submit/requestSubmit |  |
| HTML elements | button.type/disabled |  |
| HTML elements | label.htmlFor/control |  |
| HTML elements | details.open |  |
| HTML elements | style element .sheet |  |
| HTML elements | SVGElement / createElementNS svg |  |
| Events | composedPath |  |
| Events | SubmitEvent |  |
| Events | PromiseRejectionEvent |  |
| Location, History & Navigator | history.length/state |  |
| CSSOM & view | CSS.supports |  |
| CSSOM & view | CSS.escape |  |
| CSSOM & view | document.styleSheets[i].cssRules |  |
| CSSOM & view | CSSStyleSheet.insertRule |  |
| CSSOM & view | DOMMatrix |  |
| Parsing & serialization | DOMParser text/html |  |
| Crypto & performance | crypto.getRandomValues |  |
| Crypto & performance | crypto.randomUUID |  |
| Graphics & media | FontFace |  |

### BROKEN → works (9)

| Area | API | 2026-10-05 detail |
|---|---|---|
| JavaScript builtins | Date.prototype.toLocaleDateString |  |
| JavaScript builtins | Number.prototype.toLocaleString |  |
| Element | focus/blur |  |
| Observers | IntersectionObserver callback fires |  |
| Web Components | CSSStyleSheet constructable |  |
| Location, History & Navigator | history.pushState |  |
| Location, History & Navigator | history.replaceState |  |
| CSSOM & view | getComputedStyle |  |
| CSSOM & view | getComputedStyle reflects inline style |  |

### MISSING → BROKEN (2)

| Area | API | 2026-10-05 detail |
|---|---|---|
| Parsing & serialization | DOMParser image/svg+xml | threw: NotSupportedError: Failed to execute 'parseFromString' on 'DOMParser': XML documents are not supported yet. |
| Parsing & serialization | DOMParser application/xml | threw: NotSupportedError: Failed to execute 'parseFromString' on 'DOMParser': XML documents are not supported yet. |

### works → BROKEN (0)

None.


### works → MISSING (0)

None.


### BROKEN → MISSING (0)

None.


### Rows added in this rerun (84)

| Area | API | Status | Detail |
|---|---|---|---|
| Window & globals | window.outerWidth/outerHeight | works |  |
| Window & globals | window.opener/closed | works |  |
| Window & globals | screen.availWidth/colorDepth | works |  |
| Window & globals | screen.orientation | works |  |
| Window & globals | window.scroll (alias) | works |  |
| Window & globals | escape/unescape | works |  |
| Window & globals | DOMException | BROKEN | result check failed (got false) |
| Window & globals | Window/Navigator/Location/History/Screen interfaces | works |  |
| Intl | Intl.NumberFormat currency | works |  |
| Intl | Intl.NumberFormat compact | works |  |
| Intl | Intl.NumberFormat.formatToParts | works |  |
| Intl | Intl.DateTimeFormat options | works |  |
| Intl | Intl.DateTimeFormat.formatToParts | works |  |
| Intl | Intl.DateTimeFormat resolvedOptions().timeZone | works |  |
| Intl | Intl.DisplayNames | MISSING |  |
| Intl | Intl.supportedValuesOf | MISSING |  |
| Intl | Date.prototype.toLocaleTimeString | works |  |
| Intl | Date.prototype.toLocaleString (en-US, UTC) | works |  |
| Document | document.anchors/embeds | works |  |
| Document | document.fonts.ready resolves | works |  |
| Document | document.fonts.check/load | works |  |
| Document | document.fullscreenEnabled | works |  |
| Document | document.scrollingElement | works |  |
| Document | document.implementation (DOMImplementation) | MISSING |  |
| Element | Element.scrollLeft/scrollBy | works |  |
| Element | slot.assignedElements / assignedSlot | works |  |
| Element | closed shadow root hides shadowRoot | works |  |
| Element | event retargeting across shadow boundary | works |  |
| HTML elements | option.selected/defaultSelected | works |  |
| HTML elements | input.defaultChecked/indeterminate | MISSING |  |
| HTML elements | setCustomValidity/reportValidity | works |  |
| HTML elements | fieldset.elements/disabled | BROKEN | result check failed (got false) |
| HTML elements | form.length/namedItem | works |  |
| HTML elements | SubmitEvent.submitter | works |  |
| HTML elements | HTMLAnchorElement.origin | works |  |
| HTML elements | HTMLDetailsElement / details toggle | works |  |
| Events | UIEvent / CompositionEvent | works |  |
| Events | HashChangeEvent / PopStateEvent / PageTransitionEvent | works |  |
| Events | StorageEvent / ProgressEvent | works |  |
| Events | KeyboardEvent.getModifierState | works |  |
| Events | MediaQueryListEvent | works |  |
| Observers | IntersectionObserver rootMargin/thresholds | works |  |
| Observers | IntersectionObserver.takeRecords | works |  |
| Observers | IntersectionObserverEntry / ResizeObserverEntry / MutationRecord | works |  |
| Web Components | customElements.getName | works |  |
| Web Components | connected/disconnectedCallback | BROKEN | result check failed (got false) |
| Web Components | CustomElementRegistry (interface) | works |  |
| Networking | XMLHttpRequest.upload / getAllResponseHeaders | works |  |
| Networking | XMLHttpRequest synchronous open | BROKEN | threw: NotSupportedError: Failed to execute 'open' on 'XMLHttpRequest': synchronous requests are not supported |
| Networking | Request.clone / Response.clone | works |  |
| Networking | Response.error / Response.redirect | works |  |
| Networking | Response.formData (urlencoded) | works |  |
| Networking | ReadableStream.tee / pipeThrough | works |  |
| Networking | ReadableStream async iteration | works |  |
| Networking | CountQueuingStrategy / ByteLengthQueuingStrategy | works |  |
| Networking | TextEncoderStream / TextDecoderStream | works |  |
| Networking | byob reader | BROKEN | threw: NotSupportedError: Failed to execute 'getReader' on 'ReadableStream': BYOB readers are not supported. |
| URL & encoding | URL.revokeObjectURL | works |  |
| URL & encoding | URL.parse | works |  |
| URL & encoding | webkitURL alias | works |  |
| URL & encoding | URLSearchParams.size | works |  |
| URL & encoding | TextEncoder.encodeInto | works |  |
| URL & encoding | TextDecoder fatal | works |  |
| Blob, File & FormData | Blob.bytes | works |  |
| Blob, File & FormData | File.lastModified/name | works |  |
| Blob, File & FormData | FormData.entries/getAll | works |  |
| Blob, File & FormData | AbortSignal.abort / throwIfAborted | works |  |
| Location, History & Navigator | history.scrollRestoration | works |  |
| Location, History & Navigator | popstate on history.back() | works |  |
| Location, History & Navigator | pushState cross-origin throws SecurityError | works |  |
| Location, History & Navigator | navigator.plugins/mimeTypes/pdfViewerEnabled | works |  |
| Location, History & Navigator | navigator.doNotTrack/javaEnabled | works |  |
| CSSOM & view | CSSStyleSheet.replaceSync + adoptedStyleSheets | works |  |
| CSSOM & view | CSSStyleSheet.replace (Promise) | works |  |
| CSSOM & view | CSSStyleSheet.deleteRule | works |  |
| CSSOM & view | CSSStyleRule.selectorText / style.setProperty | works |  |
| CSSOM & view | <style>.sheet reflects text | works |  |
| CSSOM & view | DOMMatrix from CSS string | works |  |
| CSSOM & view | DOMMatrix.multiply/inverse | works |  |
| CSSOM & view | DOMMatrixReadOnly / WebKitCSSMatrix | works |  |
| CSSOM & view | DOMRectReadOnly / DOMPointReadOnly | works |  |
| Crypto & performance | crypto.getRandomValues rejects Float32Array | works |  |
| Crypto & performance | performance.getEntriesByName / clearMarks | works |  |
| Crypto & performance | performance.toJSON | works |  |

#### Notes on the new rows (measured, by direct follow-up calls in a throwaway test that was not committed)

- **connected/disconnectedCallback (BROKEN).** In the census context, a
  `census-life` element defined after earlier `customElements.define` calls
  got neither callback (`log` stayed empty). In a fresh bindings instance, the
  same code logs `cd` when it is the first `define`. After any earlier `define`
  (even an empty class), it logs nothing. The older `customElements.define` row
  still works because its element is the first one defined.
- **DOMException (BROKEN):** `new DOMException('m','AbortError').code` is
  `undefined`, and `name` is right. #483 states this limit.
- **fieldset.elements/disabled (BROKEN):** `fieldset.elements` is an
  `HTMLCollection` of length 0 even with an `<input>` child.
- **input.defaultChecked/indeterminate (MISSING):** `defaultChecked` is a
  boolean, and `indeterminate` is `undefined`.
- **XMLHttpRequest synchronous open, byob reader (BROKEN):** both throw
  `NotSupportedError`. These are stated limits of #455 and of `web_streams.js`.
- **document.implementation (MISSING):** `typeof document.implementation` is
  `'undefined'`. That matches the older `createHTMLDocument` row.
- **on\* handler property (old row, still MISSING):** `button.onclick` reads
  `undefined` (Chrome gives `null`), so the row classifies as MISSING.
  Measured: *assigning* `el.onclick = fn` and then calling `click()` or
  dispatching a `click` does call `fn` once. So the setter path works, and only
  the getter's initial value differs.

## Ranked top 25: still missing or broken, ordered by how much real sites depend on them

**This ranking is my judgement.** It is based on how often each API shows up in
widely used frameworks, libraries and site scripts. It was not computed from
usage data. Each entry cites the census rows behind it. These rows are left
out: the selector rows (caveat 3), the `ResizeObserver callback fires` row
(caveat 5), and the two stated-by-design limits (synchronous XHR and BYOB
readers).

| # | API (census status) | Why sites depend on it |
|---:|---|---|
| 1 | `document.implementation` and `createHTMLDocument` (MISSING) | jQuery 3.x reads `document.implementation.createHTMLDocument` in a support test while it loads. HTML sanitizers and `parseHTML`-style helpers build inert documents with it. Reading through `undefined` throws. |
| 2 | `MutationObserver` (BROKEN: the callback never fires in the 3 rows, and `takeRecords` is empty) | Used by frameworks, consent and ad scripts, polyfills, lazy widgets and devtools hooks. Anything that waits for a mutation stalls. |
| 3 | `MessageChannel`, `window.postMessage` and the window `message` event (MISSING) | React's scheduler prefers `MessageChannel`. OAuth popups, payment iframes and embeds use `postMessage`. |
| 4 | Custom elements: connected/disconnected callbacks of any element after the first `define` (BROKEN, new row) | Web-component sites define many elements (GitHub, YouTube, design systems). Only the first one would get its lifecycle callbacks. |
| 5 | `document.createTreeWalker` / `createNodeIterator` (MISSING) | Lit's template engine walks templates with `createTreeWalker`. Sanitizers (DOMPurify uses a node iterator) and focus-trap and text-highlight libraries use them too. |
| 6 | Canvas 2D: `canvas.getContext('2d')`, `CanvasRenderingContext2D`, `ImageData`, `Path2D` (MISSING) and `toDataURL` (MISSING) | Used by Chart.js-style charts, image editing, captchas and fingerprinting and bot checks. |
| 7 | `matchMedia` evaluation (BROKEN: `(min-width: 1px)` is false at 1280px) | Responsive JS, such as breakpoint hooks, carousels and menus, branches on `matchMedia(...).matches`. |
| 8 | `addEventListener({ signal })` (BROKEN: `abort()` does not remove the listener) | Modern component code and libraries remove listeners through `AbortController`. Without it, listeners leak and handlers fire twice. |
| 9 | `document.createRange` / `Range` (MISSING; the constructor throws "Illegal constructor"), `Selection.addRange` (BROKEN), `Selection.collapse/extend` (MISSING), `Range.createContextualFragment` (MISSING) | Used by rich-text editors, copy-to-clipboard helpers and HTML insertion through `createContextualFragment`. |
| 10 | `console.group/groupEnd`, `console.table/dir`, `console.time/timeEnd`, `console.assert/count/trace` (MISSING) | Production bundles sometimes leave these calls in. Calling one throws `TypeError`, and that stops the script. |
| 11 | Reflected global attributes `tabIndex/title/lang/dir` and `contentEditable/isContentEditable` (MISSING) | Accessibility code, focus management, i18n (`dir`, `lang`) and editors read and set these as properties. |
| 12 | `img.complete/naturalWidth` and `img.decode` (MISSING) | Lazy loaders, galleries, image-ready checks and masonry layouts. |
| 13 | `new EventTarget()` (BROKEN: "Illegal constructor") | Modern libraries subclass `EventTarget` as their event emitter. |
| 14 | `localStorage.foo` property access (BROKEN) | Older code and some SDKs read storage as properties, not with `getItem`. |
| 15 | `iframe.contentWindow` and `window.frameElement` (MISSING) | Used by embeds, ad slots, payment and OAuth frames, and editors with an iframe body. |
| 16 | `WebSocket`, `EventSource` (MISSING) | Used by chat, live scores, collaborative apps, notifications and dev servers. |
| 17 | `FileReader.readAsText/readAsDataURL` (MISSING) | Used by upload previews, import dialogs and image croppers. |
| 18 | `dialog.showModal/close` (MISSING) | Modern modal and dialog components. |
| 19 | `document.elementFromPoint` (MISSING) | Used by drag-and-drop, tooltips, hit-testing in editors and the "click outside" logic of overlays. |
| 20 | `Text`, `Comment`, `DocumentFragment` constructors (BROKEN: "Illegal constructor"), `Text.splitText` and `CharacterData.appendData` (MISSING) | Used by template libraries, editors and DOM utilities. |
| 21 | `Element.animate` / `getAnimations` (MISSING) | Used by animation libraries (Motion and others) and by framework transitions that use the Web Animations API. |
| 22 | `indexedDB` (MISSING), `caches` and `navigator.storage` (MISSING) | Used by Firebase and other offline SDKs, PWAs and large-app caches. These are usually feature-tested. |
| 23 | `window.onerror` property and `reportError` (MISSING) | Error-reporting SDKs and site error handlers assign `window.onerror`. |
| 24 | `on*` handler property getters (MISSING by classification: `el.onclick` reads `undefined`; the setter works, see the notes above) | Code that checks `el.onclick === null`, or `'onx' in el` style feature tests, sees the wrong value. |
| 25 | `crypto.subtle.digest` (MISSING) | Used by auth SDKs (the PKCE `S256` challenge), integrity checks and content hashing. |

**Next tier (not ranked):** these rows are also measured as missing or broken.
- **Platform:** `navigator.clipboard`, `serviceWorker`, `permissions.query`,
  `mediaDevices`, `geolocation`, `share`, `vibrate`, `connection` and
  `userAgentData`; the Navigation API; `Worker`, `SharedWorker` and
  `BroadcastChannel`; `Notification`, `speechSynthesis`, `AudioContext`,
  `MediaSource`, WebGL and `OffscreenCanvas`; `video`/`audio` `play()`.
- **Parsing:** `DOMParser` for XML types (BROKEN: "XML documents are not
  supported yet"), `XMLSerializer`, `Element.setHTMLUnsafe` and
  `TextDecoder('windows-1252')` (BROKEN).
- **Forms:** `fieldset.elements` (BROKEN), `input.indeterminate` (MISSING) and
  `DataTransfer` constructor (BROKEN).
- **Document and element:** `document.domain` (BROKEN), `execCommand`,
  `startViewTransition`, `table.insertRow`, `link rel=stylesheet` `.sheet`,
  `checkVisibility`, `popover`, `requestFullscreen`, `setPointerCapture`,
  `computedStyleMap`, `attachInternals` and `ReportingObserver`.
- **JavaScript and Intl:** `Intl.Segmenter`, `Intl.Locale`, `Intl.DisplayNames`
  and `Intl.supportedValuesOf` (MISSING); `FinalizationRegistry`;
  `DOMException.code`.
- **Scheduling and view:** `scheduler.postTask`, `CompressionStream`,
  `visualViewport` and `cookieStore`.

## What was not measured

- **The engine's selector matcher.** Selector APIs here use the bindings'
  fallback matcher (caveat 3).
- **Layout-dependent values.** There is no layout in this harness. The geometry
  APIs are now present, but whether their numbers are correct, and whether
  ResizeObserver callbacks are delivered, were not measured (caveat 5).
- **Real network behaviour** of `fetch`/XHR/`sendBeacon`: there is no network
  here.
- **Event delivery driven by the engine**, such as real input, focus, scroll and
  hover. #480 to #527 change these engine paths, which do not build on Linux.
- **Anything specific to macOS, WebKit-fallback, or the chrome/shelf WebViews.**
- **Presence-only rows (`*`):** whether the call actually works.
- **Spec conformance beyond one smoke call.** The WPT feasibility study in the
  [2026-10-03 census](WEB_API_CENSUS_2026-10-03.md#feasibility-running-upstream-wpt-testharnessjs-tests-in-this-harness)
  was not rerun.
