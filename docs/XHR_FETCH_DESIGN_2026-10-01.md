# XMLHttpRequest and fetch: design

Status: PROPOSED, for Prometheus R1 and Pete's sign-off before any code PR.
Author: Athena (JS surface owner). Network-layer owner offered: Talos.
Requested by: Atlas #577 (live-site JS lane). Atlas #578 set the required
additions (A)-(F) below; Talos #295 supplied the connect-site fact in §2.

## 1. Why, and why not as a plain stub

`XMLHttpRequest` is the largest remaining root cause in the live-site script
logs (4 throwing scripts on the 20-site board, and every API-driven page needs
it or `fetch`; `fetch`, `Headers`, `Request`, `Response` are also absent).
Many of the "cannot convert null or undefined" throws downstream are cascades
from a consent or data script that died on the first `XMLHttpRequest`.

It cannot ship as a stub. Page script is untrusted and runs inside a browser
that holds user data and on fleet machines that run `parity-capture` against
live sites while local services listen (MCP servers, the null daemon, dev
servers). Script-initiated network access is where the same-origin policy and
the private-network boundary are enforced; a wrong design here is a security
bug, not a parity gap. So: design first, deny-path tests first, and the JS
surface is not installed until the policy exists.

## 2. What exists today (verified in develop)

- One entry point: `ResourceLoader::fetch(Request)` in `rustkit-net`. The
  shield interceptor (`InterceptHandler`, EasyList, the 24h list cache,
  `HIWAVE_SHIELD_OFF`) runs at the top of it, then `data:` handling, then the
  HTTP cache, then `rustkit-http`. Anything that goes through it gets the
  shield.
- `Request` carries `credentials: CredentialsMode` (`Omit` / `SameOrigin` /
  `Include`), `referrer`, `referrer_policy`, and `destination:
  RequestDestination` (`Document`/`Style`/`Script`/`Image`/`Font`/`Other`).
  Script requests would use `Other` today; the shield classifies `Other`
  without type-specific rules. A `Fetch`/`Xhr` destination is wanted (§4).
- `rustkit-net/src/security.rs` already defines `Origin`, `CorsChecker`
  (`is_simple_request`, `check_response`, `parse_preflight_response`),
  `check_mixed_content`, `ContentSecurityPolicy::allows_connect` and
  `CookieAttributes`. **None of it is called from the engine or the loader's
  request path**; it is defined and unit-tested, not enforced.
- `rustkit-http` has **no cookie store** (`cookie_store()` is a documented
  no-op), so requirement (E) holds today by construction: no cookies are sent.
- `rustkit-http` connects with `TcpStream::connect(&addr)` on a `host:port`
  string at four sites (`crates/rustkit-http/src/lib.rs` lines 389, 541, 981,
  1013), so **name resolution happens inside `connect`**. A pre-check on the
  hostname string cannot satisfy (A): a name that resolves to `127.0.0.1`
  would pass it. (Talos #295.)
- Redirects: followed inside `rustkit-http` up to `max_redirects` (10). The
  loader and the shield appear to see only the first URL; the net-layer owner
  should confirm that before (D) is designed against it.
- Script timing: page scripts run once after the document is parsed, then
  lifecycle events and timers on a virtual clock
  (`Engine::run_page_scripts`, `DomBindings::run_timers`), inside
  `script_budget_ms`.

## 3. Threat model and required rules

Required by Atlas #578; each is enforced in one named place and has a
deny-path test (§7).

| | Rule | Enforced in |
|---|---|---|
| A | **Private-network block.** A page whose origin is public may not reach loopback (`127.0.0.0/8`, `::1`, `localhost`), RFC1918, link-local (`169.254/16`, `fe80::/10`), unique-local (`fc00::/7`), unspecified, or CGNAT/benchmark ranges. Checked on the **resolved IP**, before connecting, and again on every redirect hop. Resolve once, vet every returned address, connect to the vetted `SocketAddr`. | net layer (`rustkit-http`), all 4 connect sites |
| B | **No side door.** Every script request is a `Request` through `ResourceLoader::fetch`: the shield, cache and `HIWAVE_SHIELD_OFF` behave as for subresources. | engine glue |
| C | **Mixed content.** An https page cannot fetch http (blocked, not upgraded). | net policy (`check_mixed_content`, newly called) |
| D | **Redirects.** Re-run origin, CORS, private-network and scheme checks on every hop; hop cap; drop credentials on a cross-origin redirect; method/body rules per status. | net layer (the loader must see hops) |
| E | **Cookies.** No cookies from script requests at all until a partitioned jar exists; credentials mode degrades to `omit`. Stated in the PR, not half-implemented. | net policy (and true today) |
| F | Deny-path tests first (§7). | all |
| S | **Same-origin policy and CORS.** Same-origin: allowed. Cross-origin: allowed only when the response passes CORS (simple request: `Access-Control-Allow-Origin` equals the page origin or is `*`, and `*` is rejected when credentials are included; non-simple method or header: a preflight `OPTIONS` first, `Access-Control-Allow-Methods/Headers` honoured, cache keyed per origin+URL). A denied response is **never exposed to script** (XHR `error` event, fetch `TypeError`); `no-cors` fetch returns an opaque response (type `opaque`, status 0, empty body). | net policy (`CorsChecker`, newly called) |
| T | **Schemes.** `http`/`https` only for network; `data:` allowed for fetch (same-origin-opaque, already handled in the loader); `file:` never from a web origin; no `blob:` networking in this work. | net policy |
| U | **CSP `connect-src`.** If the page carries a CSP, `allows_connect` gates the request. | net policy (newly called) |
| V | **Budgets.** Max in-flight (6 per origin, 24 per page), max total requests per page (500), per-request timeout (30 s, cut to the remaining script budget), response cap (10 MB, then abort), request body cap (10 MB), header count/size caps. All inside the existing script budget so a page cannot hang a load. | engine glue + net layer |

Not in scope and said so: cookies and credentialed cross-origin (E),
`WebSocket`, `EventSource`, service workers, `sendBeacon` (stays a no-op),
streaming response bodies (the body is delivered whole; `ReadableStream` is a
separate item).

## 4. Architecture: three layers, owned separately

```
  page script                 bindings (Athena)         engine glue (Athena)          net policy (Talos)
  XMLHttpRequest / fetch  ->  queue ScriptRequest   ->  drain queue after scripts ->  FetchPolicy::vet(req)
  Headers/Request/Response    (no network access)       and each timer round,        resolve+vet IPs, scheme,
                              deliver ScriptResponse    through ResourceLoader        mixed content, CORS,
                              events / promises         (shield, cache, interceptor)  redirects, caps
```

**Interface (the contract between the layers).**

```
ScriptRequest  { id, method, url, headers, body: Option<Bytes>,
                 mode: Cors | NoCors | SameOrigin,
                 credentials: Omit | SameOrigin | Include,   // degrades to Omit (E)
                 redirect: Follow | Manual | Error,
                 initiator: { page_url, page_origin, csp } }
ScriptResponse { id, url, status, status_text, headers (filtered), body,
                 kind: Basic | Cors | Opaque | Error(reason) }
```

- **Net layer** (proposed: Talos, `rustkit-net` / `rustkit-http`). A
  `FetchPolicy` that takes a `ScriptRequest` and either returns a vetted
  `Request` plus a response filter, or a deny. It owns: resolve-vet-connect at
  the four sites, per-hop redirect handling that surfaces each hop to the
  policy, `check_mixed_content`, `CorsChecker` incl. preflight and its cache,
  CSP `connect-src`, the caps, and response filtering (CORS-safelisted headers
  only for `cors`; opaque for `no-cors`). It is the only layer that decides
  allow/deny. It adds `RequestDestination::{Fetch, Xhr}` so the shield can
  classify.
- **Engine glue** (Athena). After the page scripts and after each timer round
  the engine takes the queue, hands each request to `FetchPolicy`, fetches
  through `ResourceLoader::fetch` (B), applies the budgets (V), and delivers
  each result back to the bindings, then runs the timers again (a response
  handler may schedule more work). Bounded rounds (default 8) inside
  `script_budget_ms`; whatever is still pending at the end completes with a
  network error. The queue is drained in request order and delivered in id
  order so runs are deterministic.
- **JS surface** (Athena, `rustkit-bindings`). `XMLHttpRequest` (states 0-4,
  `open`/`setRequestHeader`/`send`/`abort`, `onreadystatechange`, `onload`,
  `onerror`, `onabort`, `ontimeout`, `onloadend`, `timeout`, `responseType`
  `''|text|json|arraybuffer|blob`, `response`/`responseText`/`status`/
  `statusText`/`getResponseHeader`/`getAllResponseHeaders`, `withCredentials`,
  forbidden request headers refused, `upload` stub), and `fetch`, `Headers`,
  `Request`, `Response` (`json`/`text`/`arrayBuffer`/`blob`/`clone`, `ok`,
  `redirected`, `type`, `AbortSignal` support using the merged `AbortController`).
  The surface has **no network access of its own** and is **only installed when
  the engine registers the bridge**: a bindings instance without the engine
  glue (and so without the policy) has no `XMLHttpRequest`/`fetch` at all,
  never a permissive one.

## 5. Why this split

- The allow/deny logic lives in one place that already owns connections and
  that the macOS network lane reviews; the JS layer cannot weaken it.
- The deny-path tests exercise `FetchPolicy` and the real connect sites with a
  localhost test server (the engine tests already have one), independent of any
  JS.
- The JS surface can be written, reviewed and tested against a fake bridge
  (a recording queue) with no network at all.

## 6. Open decisions for Pete

1. **Cookies (E).** Confirm: script requests send none until a partitioned jar
   exists. (Today the whole stack sends none, so this is a statement of
   intent, not a regression.)
2. **Budget numbers** in rule V (6/24 in flight, 500 total, 30 s, 10 MB).
3. **Mixed content**: block (proposed) or upgrade; blocking is the stricter
   default and matches Chrome for XHR/fetch.
4. **Private network for a private page.** A page that is itself on a private
   address (a dev server) may reach private addresses of the same origin only;
   confirm, since the fleet must not be able to use that to reach a neighbour.

## 7. Deny-path tests (written and failing first)

Net layer, against the localhost test server and a resolver stub:

1. public-origin page -> `http://127.0.0.1:PORT/` is denied; also `localhost`,
   `[::1]`, `0.0.0.0`, `169.254.169.254`, `10.0.0.1`, `192.168.1.1`,
   `172.16.0.1`, `100.64.0.1`.
2. **a name that resolves to 127.0.0.1** (resolver stub returns it) is denied,
   and the denial happens before any socket is opened (test asserts no
   `accept`). A name returning one public and one private address is denied.
3. **redirect to a private IP**: public URL answers `302 Location:
   http://127.0.0.1:PORT/`; denied at the hop, the private server sees no
   connection. A cross-origin redirect drops credentials. Hop cap trips.
4. https page -> `http://` target is blocked (mixed content); `https://` allowed.
5. cross-origin without `Access-Control-Allow-Origin` -> error, body not
   exposed; with the right origin -> allowed; `*` plus credentials -> denied;
   non-simple (`PUT`, custom header) -> preflight sent, denied when the
   preflight lacks the method/header, allowed when it has them.
6. `no-cors` fetch -> opaque (status 0, empty body).
7. response over the cap is aborted; request over the in-flight cap queues;
   the per-request timeout fires; total budget exhaustion errors the rest.
8. the shield blocks an EasyList-matching script request exactly as it blocks
   the same subresource; `HIWAVE_SHIELD_OFF` disables both.
9. `file:` / `ftp:` / `javascript:` schemes denied.

Engine glue and JS: a fake bridge records and answers requests; XHR state
machine and event order, `fetch` promise/`Response` semantics, abort, timeout,
`responseType`s, forbidden headers, and "no `XMLHttpRequest` when no bridge".

## 8. Sequence and ownership

1. **This document** (PR 1): Prometheus R1 against Atlas #578 (A)-(F); Pete's
   sign-off on §6.
2. **Net policy** (PR 2, proposed Talos; Atlas assigns): `FetchPolicy`, the
   resolve-vet-connect change at the four sites, per-hop redirect surfacing,
   CORS/mixed-content/CSP wiring, caps, `RequestDestination::{Fetch, Xhr}`,
   and the §7 net tests. Lands with the policy callable but **nothing in the
   engine calling it yet**.
3. **Engine glue + bridge** (PR 3, Athena): queue drain after scripts and timer
   rounds, budgets, determinism, and the fake-bridge tests.
4. **XHR surface** (PR 4, Athena), then **fetch/Headers/Request/Response**
   (PR 5, Athena), each with before/after from the live script logs.

The Windows seat carries all of it unchanged (cross-platform crates); the only
Windows-specific item is checking that the resolve step uses the platform
resolver and that the 16 MB main-thread stack (hiwave-windows #104) holds under
the engine glue's recursion.
