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
| `facebook` | fail | fail | 0.68% | 0.00% | `none` (silent / no throw recorded; 48 scripts ran) |
| `github` | fail | fail | 13.69% | 0.00% | `TypeError: cannot convert 'null' or 'undefined' to object` |
| `google` | fail | fail | 1.82% | 0.00% | `TypeError: cannot convert 'null' or 'undefined' to object` |
| `instagram` | fail | fail | 0.74% | 0.00% | `none` (silent / no throw recorded; 57 scripts ran) |
| `linkedin` | fail | fail | 12.01% | 0.00% | `Error: Please pass a valid pageKey, counterMetricEndpoint & gaugeMetricEndpoint` |
| `lyft` | fail | fail | 20.84% | 0.00% | `JS engine panic` (`pages/_app-2af4fc172a4b9314.js`) |
| `microsoft` | fail | fail | 24.98% | 0.00% | `none` (silent / no throw recorded; 255 scripts over budget) |
| `netflix` | fail | fail | 0.28% | 0.00% | `TypeError: not a callable function` |
| `reddit` | fail | fail | 5.19% | 0.00% | `none` (silent / no throw recorded; 1 script ran) |
| `shopify` | **PASS** | **PASS** | 52.41% | 96.30% | `none` (responsive link navigation) |
| `squarespace` | fail | fail | 96.22% | 0.00% | `TypeError: not a callable function` |
| `walmart` | fail | fail | 0.25% | N/A | `Stack overflow in Akamai bot script (thread main overflowed its stack)` |
| `weather` | fail | fail | 7.93% | 0.00% | `none` (silent / no throw recorded; 157 ran, 1 nomodule skipped) |
| `wikipedia` | fail | fail | 0.95% | 0.01% | `none` (slight movement 0.01%, below 0.50% threshold; 4 scripts ran) |
| `x` | fail | fail | 0.39% | 0.00% | `none` (silent / no throw recorded; 22 ran, 1 nomodule skipped) |
| `yahoo` | fail | fail | 20.20% | 0.00% | `none` (silent / no throw recorded; 53 ran, 1 nomodule skipped) |
| `youtube` | fail | fail | 82.15% | 0.00% | `TypeError: not a callable function` |

---

## 2. Ranked List of Root Blockers Across the 19 Failing Sites

> **Note on Chrome Verification:** The script errors below reflect the first unhandled JS error recorded by RustKit (`outcome == "threw"`). They have **not yet been diffed against Chrome's `page-errors.json`**. Production sites frequently throw non-fatal errors in Chrome as well. Therefore, script errors listed in Ranks 2, 3, 5, and 6 represent working hypotheses rather than confirmed root blockers until cross-verified against Chrome console error logs.

| Rank | Frequency | Blocker Category | Sites | Forensics & Working Hypothesis |
|---|---|---|---|---|
| **1** | **8 / 19** (42.1%) | **Silent / No Throw Recorded** | `facebook`, `instagram`, `microsoft`, `reddit`, `weather`, `wikipedia`, `x`, `yahoo` | Scripts execute cleanly without throwing an uncaught exception (`ran` cleanly or skipped `<script nomodule>` as expected for module-capable engines; `microsoft` scripts exceeded the run budget). However, user click does not invoke a responsive UI state or visual paint change in RustKit (e.g. missing event listener attachment, unsupported UI event type, or inert DOM mutation). On `wikipedia`, a 0.01% delta was registered. |
| **2** | **4 / 19** (21.1%) | **`TypeError: cannot convert 'null' or 'undefined' to object`** *(unverified vs Chrome)* | `apple`, `cnn`, `github`, `google` | Attempting property access or `Object.keys()`/destructuring on an uninitialized/unsupported DOM property during page init, potentially preventing event listener registration. |
| **3** | **3 / 19** (15.8%) | **`TypeError: not a callable function`** *(unverified vs Chrome)* | `netflix`, `squarespace`, `youtube` | Web API or constructor invoked by site scripts is a dummy object or undefined stub rather than a callable function (e.g. YouTube `webcomponents-sd.js` #8). |
| **4** | **2 / 19** (10.5%) | **Engine Panic / Stack Overflow** | `lyft`, `walmart` | Engine execution failures:<br>• `lyft`: JS engine panic evaluating Next.js bundle `pages/_app-2af4fc172a4b9314.js`.<br>• `walmart`: Complex obfuscated recursion in Akamai bot script overflows thread stack on Windows (`thread 'main' has overflowed its stack`). |
| **5** | **1 / 19** (5.3%) | **`ReferenceError: _w is not defined`** *(unverified vs Chrome)* | `bing` | Bing inline bootstrap script expects global `_w` (window alias) to be defined prior to search widget attachment. |
| **6** | **1 / 19** (5.3%) | **Config / Validation Error** *(unverified vs Chrome)* | `linkedin` | Cloudflare/LinkedIn telemetry bootstrap fails with `Please pass a valid pageKey, counterMetricEndpoint & gaugeMetricEndpoint`. |
