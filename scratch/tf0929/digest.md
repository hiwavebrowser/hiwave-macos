
## 2026-09-29 11:30 — #346 (merged 09:54) slowed every HTTPS load: about 10 s per capture plus a cert reload per h2 connection; fix pushed as `atlas/rs-tls-roots-once`. #358 opened (translate/rotate/scale + token-safe var())

**Points: 26 → no full board this session.** The board slot went to #346's A/B. #346 merged mid-run, and on the 10 sites both arms finished it scored **7/30 vs develop 062f73a's 16/30**, all of it timeouts and slow loads. Develop (d021be5) now carries that regression, so the next develop board will fall until the fix lands. Per-check totals on those 10 sites: develop loads 8, readable 4, looks-right 4; #346 loads 4, readable 2, looks-right 1.

| site | develop 062f73a | #346 32a227e | RustKit capture time dev → #346 |
|---|---|---|---|
| x | 3 | 0 (timeout) | 3.9 s → >30 s |
| wikipedia | 2 | 0 (timeout) | 7.1 s → >30 s |
| facebook | 2 | 0 (timeout) | 29.0 s → >30 s |
| yahoo | 1 | 0 (timeout) | 17.9 s → >30 s |
| linkedin | 3 | 2 | 13.0 s → 22.3 s |
| google | 2 | 2 | 4.6 s → 18.0 s |
| instagram, lyft, reddit, youtube | same | same | 9.8→25.3, 17.0→28.2, 0.9→10.4, 7.2→13.0 s |

Run dirs: `trench/realsite/runs/20260929T1420Z-tls346-{dev,pr346}` (chunks 0–1 complete; stopped in chunk 2 once #346 had merged). Reproduced outside the board, alternating arms: reddit 1.0/1.0 s on develop vs 9.3/11.1 s on #346; google 19.1/11.7 vs 36.6/30.9 s (load ~15).

**Cause (from the diff and a verbose capture):** `rustls_native_certs::load_native_certs()` walks the macOS keychain (seconds). #346 calls it in every `Client::with_config`, which costs +5.2 s at engine start since the engine builds two clients. It calls it again inside `connect_tls` on **every connection whose server picks h2**, where it rebuilds the http/1.1-only config, and reddit's top-level GET went from 0.14 s to 5.45 s. **Fix pushed, NO PR yet: `atlas/rs-tls-roots-once` @ 307fc8e** (from develop d021be5). The store is a process-wide `OnceLock`, and the h1-only connector is built once per client. The TLS profile, ALPN offer and downgrade are unchanged. It has a pin (3 clients → 1 store load), and rustkit-http 11/11 and rustkit-net 41/41 pass. TIMING_LINE

**PRs:**
- **#358 `atlas/rs-individual-transforms` @ 1d1c972** (new). (1) `translate`/`rotate`/`scale` as their own properties, composed ahead of `transform` per css-transforms-2 §6. (2) `var()` substitution no longer fuses tokens. Tailwind v4's `translate:var(--tw-translate-x)var(--tw-translate-y)` had spliced to the invalid `0-200%`, so every Tailwind v4 translate/scale utility was dropped. **shopify LOOKS RIGHT 25.9/25.9% → 24.0/24.2%** (2 interleaved rounds, Chrome-vs-Chrome 0.0%). The skip link now sits at -88 px, as in Chrome, and the page has 71 transforms, up from 9. Campaign 26/26 identical to develop 062f73a, ratchet output byte-identical, engine 270/270 serial, 6 pins (each fix's pins fail first). WPT not run: `third_party/wpt` isn't synced in that worktree. No full-board A/B yet.
- #347 and #354: merged develop in additively (6c63121, 3cab84e; both conflicts were test modules appended at the same spot). **Both MERGED** later this morning, along with #346.
- Commenting on PRs needs approval on this seat, so the #346 receipt and the merge notes are here instead.

**Next:** (1) Receipts for `atlas/rs-tls-roots-once` (time_arms on reddit/google/x plus a board chunk), then the PR. It's the biggest point-mover on develop right now. (2) A full develop board after it lands. (3) facebook's second flex cause. (4) Talos 5 audit.

**Decisions for Pete:**
1. **#346 merged without its board receipt** (my #535 condition), and it costs about 9 points on the 10 sites measured. Revert #346 until `rs-tls-roots-once` lands, or fast-track the fix? The fix is small and keeps Talos's design. Talos should also know that the h2 → http/1.1 re-handshake still costs an extra TCP+TLS round trip per connection to h2 origins.
2. Still open: a fixed ~06:00 board slot before the lanes start; `<img>` through the ResourceLoader; `cargo fmt` on develop.
