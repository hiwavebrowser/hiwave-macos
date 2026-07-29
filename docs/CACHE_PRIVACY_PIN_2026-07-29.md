# Cache privacy pin — what must be settled before C2 wires MemoryCache

**Owner:** Atlas (pathfinder) · **Date:** 2026-07-29
**Requested by:** Prometheus — *"C2 HOLD … Atlas must pin: defaults verbatim?,
partition/credentials, opt-out for tests/private, stats surface"*, escalated
to *"live macos defects; module-identical ports re-introduce them. Atlas fixes
macos first; then privacy pin; then wire."*

Athena and Talos must not wire `ResourceLoader ← MemoryCache` on their trees
until §2 is decided. §1 is already fixed on macOS and should be ported with
the module.

---

## 0. One correction to the tasking

The hold implied `no-store` handling was among the live defects. **It is not.**
`parse_cache_control` maps `no-store` and `no-cache` to `Duration::ZERO`, and
the loader stores only when `ttl > Duration::ZERO`, so those responses are
never cached. Verified and now pinned by a test so a TTL refactor cannot
quietly lose it.

Saying so because a hold that names the wrong defect gets lifted for the wrong
reason.

---

## 1. Fixed on macOS (port with the module, no design needed)

Both found by asking who *reads* a field rather than who declares it — the
same method that found the parity `instrumentFailure` bug the same day.

| Field | References before | Reality |
|---|---|---|
| `CacheConfig::default_ttl` | 3 — declaration, default, **startup log** | read by nothing |
| `CacheConfig::respect_cache_control` | 2 — declaration, default | **zero readers** |

- **The loader used the wrong config field entirely.** Its fallback TTL was
  `LoaderConfig::default_timeout` — a *network request timeout* of 30s — so a
  response with no `Cache-Control` was cached for 30s while the cache's own
  `default_ttl` of 300s was announced in the startup log and applied nowhere.
  A number printed at boot and used nowhere is worse than no number: it reads
  as configuration.
- **`respect_cache_control` was decorative.** Setting it `false` changed no
  behaviour, because no code path consulted it.

Both now flow through the cache's own accessors, with regression tests.

---

## 2. NOT fixed — needs a decision before any tree wires this

### 2.1 The cache is unpartitioned (the headline)

```rust
pub struct CacheKey {
    pub url: String,
    pub method: String,
}
```

There is no top-level-site component. A resource fetched while browsing site A
is served from cache to site B, and the *timing* of that hit is observable
from script. That is the cross-site cache leak every major engine closed years
ago — Safari first, Chrome 85, Firefox 85 — and it is a history-disclosure
channel, not a performance detail.

For a browser whose entire pitch is privacy-first, shipping an unpartitioned
cache is the single most quotable defect in the network stack.

**Decision needed:** partition key shape. The industry default is
(top-level site, resource URL), sometimes plus a cross-site frame bit. That
requires the loader to know its top-level site, which it currently does not —
so this is an API change through `ResourceLoader`, not a patch to `CacheKey`.

**Recommendation:** double-keyed on (top-level eTLD+1, url, method). Accept the
hit-rate cost; it is the cost of the product's own claim.

### 2.2 `Vary` is ignored

A response varying on `Accept`, `Accept-Language`, or `Accept-Encoding` is
stored under a key that records none of them, so the next request with
different headers gets the wrong body. Correctness first, privacy second — but
`Vary: Cookie` makes it both.

**Decision needed:** honour `Vary` by folding the named request headers into
the key, or refuse to cache any response carrying `Vary` at all. The second is
cheaper, more conservative, and I would take it until there is a measured
reason not to.

### 2.3 Credentialed responses are cached like any other

Nothing inspects the request for `Authorization`, and nothing inspects the
response for `Cache-Control: private`. An authenticated response is cached and
served from a key that does not include the credential.

**Decision needed:** skip caching when the request carried `Authorization`, and
treat `private` as non-cacheable in the shared in-memory cache.

### 2.4 There is no opt-out

`MemoryCache` is unconditional — `ResourceLoader::new` always builds one. A
private-browsing mode, or a test that wants determinism, has no way to disable
it, and `clear()` exists but nothing calls it on a context boundary.

**Decision needed:** a `CacheConfig::enabled` flag honoured at the loader, plus
a defined lifecycle point where a private context clears or refuses the cache.

---

## 3. Sequence

1. ~~§1 dead-config fixes on macOS~~ — done, this PR.
2. **§2 decisions.** 2.1 is the one that needs Pete: it trades measurable
   performance for the privacy claim the product is sold on, which is a
   product call rather than a technical one. 2.2–2.4 I can take.
3. Only then: Athena/Talos wire C2 on their trees.

Porting the module today without §2 propagates an unpartitioned cross-site
cache to three platforms instead of one. That is the whole reason for the hold,
and it is a good reason.
