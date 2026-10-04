# Develop Tip Interactive Rerun & Script Error Forensics (@ 5744c7ce)

**Date:** 2026-10-04  
**Seat:** `pollux` (Windows seat)  
**Binary:** Pinned release build of `parity-capture.exe` on `develop` tip `5744c7ce`  
**Landed Patches Evaluated:** #480 (click delivery), #498 (inline onclick), #500 (button/image click target), #502 (checkbox/radio activation), #504 (submit button), #494/#501/#505 (geometry, computed style, layout between lifecycle steps).

---

## 1. Before / After Comparison Table

| Site | Baseline (6eb6a47c) | Develop Tip (5744c7ce) | Chrome Delta | RustKit Delta | First Script Error / Blocker |
|---|---|---|---|---|---|
| `apple` | fail | fail | 23.07% | 0.00% | `TypeError: cannot convert 'null' or 'undefined' to object` |
| `bing` | fail | fail | 83.58% | 0.00% | `ReferenceError: _w is not defined` |
| `cnn` | fail | fail | 47.78% | 0.00% | `TypeError: cannot convert 'null' or 'undefined' to object` |
| `facebook` | fail | fail | 0.68% | 0.00% | `none` (silent / missing handler) |
| `github` | fail | fail | 13.69% | 0.00% | `TypeError: cannot convert 'null' or 'undefined' to object` |
| `google` | fail | fail | 1.82% | 0.00% | `TypeError: cannot convert 'null' or 'undefined' to object` |
| `instagram` | fail | fail | 0.74% | 0.00% | `none` (silent / missing handler) |
| `linkedin` | fail | fail | 12.01% | 0.00% | `Error: Please pass a valid pageKey, counterMetricEndpoint & gaugeMetricEndpoint` |
| `lyft` | fail | fail | 20.84% | 0.00% | `nomodule (skipped, as in module-capable browsers)` |
| `microsoft` | fail | fail | 24.98% | 0.00% | `none` (silent / missing handler) |
| `netflix` | fail | fail | 0.28% | 0.00% | `TypeError: not a callable function` |
| `reddit` | fail | fail | 5.19% | 0.00% | `none` (bot challenge link navigation inert) |
| `shopify` | **PASS** | **PASS** | 52.41% | 96.30% | `none` (responsive link navigation) |
| `squarespace` | fail | fail | 96.22% | 0.00% | `TypeError: not a callable function` |
| `walmart` | fail | fail | 0.25% | N/A | `Stack overflow in Akamai bot script (thread main overflowed its stack)` |
| `weather` | fail | fail | 7.93% | 0.00% | `nomodule (skipped, as in module-capable browsers)` |
| `wikipedia` | fail | fail | 0.95% | 0.01% | `none` (slight movement 0.01%, below 0.50% threshold) |
| `x` | fail | fail | 0.39% | 0.00% | `nomodule (skipped, as in module-capable browsers)` |
| `yahoo` | fail | fail | 20.20% | 0.00% | `nomodule (skipped, as in module-capable browsers)` |
| `youtube` | fail | fail | 82.15% | 0.00% | `TypeError: not a callable function` |

---

## 2. Ranked List of Root Blockers Across the 19 Failing Sites

| Rank | Frequency | Blocker Category | Sites | Root Cause & Forensics |
|---|---|---|---|---|
| **1** | **5 / 19** (26.3%) | **Silent / Missing Event Handler** | `facebook`, `instagram`, `microsoft`, `reddit`, `wikipedia` | Scripts execute cleanly without exception, but user click does not invoke responsive UI state or visual paint change in RustKit (e.g. focus border, ripple, or modal trigger). On `wikipedia`, a 0.01% delta was registered. |
| **2** | **4 / 19** (21.1%) | **`TypeError: cannot convert 'null' or 'undefined' to object`** | `apple`, `cnn`, `github`, `google` | Attempting property access or `Object.keys()`/destructuring on an uninitialized/unsupported DOM property during page init, preventing event listener registration. |
| **3** | **4 / 19** (21.1%) | **`nomodule` skipped / ES module load failure** | `lyft`, `weather`, `x`, `yahoo` | Modern bundles delivered via `<script type="module">` fail or get skipped, and the fallback `<script nomodule>` is ignored as in modern module-capable browsers, leaving elements without attached interaction listeners. |
| **4** | **3 / 19** (15.8%) | **`TypeError: not a callable function`** | `netflix`, `squarespace`, `youtube` | Web API invoked by site scripts is a dummy object or undefined stub rather than a callable function. |
| **5** | **1 / 19** (5.3%) | **`ReferenceError: _w is not defined`** | `bing` | Bing inline bootstrap script expects global `_w` (window alias) to be defined prior to search widget attachment. |
| **6** | **1 / 19** (5.3%) | **Config / Endpoint Validation Error** | `linkedin` | Cloudflare/LinkedIn telemetry bootstrap fails with `Please pass a valid pageKey...`. |
| **7** | **1 / 19** (5.3%) | **Akamai Stack Overflow on Windows** | `walmart` | Complex obfuscated script recursion overflows the thread stack in RustKit engine on Windows. |
