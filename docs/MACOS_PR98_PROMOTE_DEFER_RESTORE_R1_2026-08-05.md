# Outside-eye R1 — hiwave-macos PR #98 (promote develop→master: deferred restore navigation)

**Date:** 2026-08-05  
**Seat:** Prometheus (Grok, design-only)  
**PR:** https://github.com/hiwavebrowser/hiwave-macos/pull/98  
**Tip / head:** `6c7ef42` (origin/develop = Merge #97)  
**Base:** master `7b73635` (Merge #96)  
**Cumulative product commit:** `df2bd87` fix(app): defer restored-tab navigation until after first paint  
**Verdict:** **DESIGN CLEAR / APPROVE promote** develop→master @ `6c7ef42`  
**Merge authority:** Atlas / Pete (master write). Prometheus does **not** merge.

---

## 1. Why this unit (queue rule)

Prior tick banked:

- #97 **product CLEAR for develop** once CI green (content R1)
- **Promote develop→master HOLD** because state-line claimed #97 already on develop when it was not, and swarm was still in progress

Live re-measure this tick:

| Surface | State |
|---------|-------|
| macOS **#98** | OPEN · MERGEABLE · CLEAN · head `6c7ef42` · base `7b73635` · **NEW promotion residual** |
| macOS **#97** | **MERGED** 2026-08-05T10:38:46Z → develop @ `6c7ef42` |
| macOS master | `7b73635` |
| macOS develop | `6c7ef42` (ahead 2 / behind 0 vs master) |
| Linux #58 | OPEN · CLEAR banked @ `387a8ee` |
| Win open | #33 HOLD only · master `f12fd9d` · develop `e98b818` (tools #76/#77 ahead; not product residual for this unit) |
| umbrella #11 | OPEN · HARD AMEND banked |
| community #2/#3 | **MERGED** |
| tank | open zero · main `85ce800` |

#98 is the only new outside-eye residual this tick.

---

## 2. State-line correction vs prior Atlas claim

| Claim (prior) | Measured now |
|---------------|--------------|
| "#97 self-merged to develop" | **TRUE** — merged; tip is merge commit `6c7ef42` |
| "cumulative diff = exactly one commit" | **SOFT** — `master..develop` is **2** commits (`df2bd87` product + `6c7ef42` merge). Product delta = **one commit**. Acceptable under merge-PR model; do not treat as second feature. |
| CI green | **TRUE** on #98 — audit + pr-swarm 0..3 + pr-aggregate all **SUCCESS** (commit-gate / nightly skipped as expected) |
| Something to promote | **TRUE** — not a no-op; master..develop non-empty |

Prior HOLD on promote is **lifted** for conditions (a)(b)(c). Condition (d) Pete direct go for master still required (exchange is discounted trust; Prometheus does not flip the switch).

---

## 3. Independent ground — cumulative product

### Scope

| Path | Δ |
|------|---|
| `crates/hiwave-app/src/main.rs` only | +17 / −5 |

No engine, no shell config, no docs, no harness.

### Master defect (CONFIRMED)

On master `7b73635`, rustkit cfg block after content webview build:

```text
if !is_new_tab_url(&initial_url) {
    if let UnifiedContentWebView::RustKit(ref v) = content_webview {
        v.load_url(&initial_url)  // SYNCHRONOUS, before event_loop.run
    }
}
```

Ordering:

1. setup builds Four-WebView + ABOUT chrome HTML
2. **blocks** on restore load (`load_url` → blocking path → network/parse/layout)
3. only then reaches `event_loop.run` (~L1890)

Coherent with live process-sample root cause (main thread parked; zero paints).

### Tip fix (ACCEPT)

Replace direct `load_url` with:

```text
info!(url = %initial_url, "Deferring restored-tab navigation until after first paint");
let _ = proxy.send_event(UserEvent::Navigate(initial_url.clone()));
```

Ordering proof on develop tip:

| Step | Site |
|------|------|
| `proxy = event_loop.create_proxy()` | ~L606 |
| content webview built | before L1566 |
| `send_event(Navigate)` | ~L1585 |
| `event_loop.run` | ~L1890 |

Proxy queue is live before `run`; event is handled on first UserEvent pump after loop start. Window/chrome exist before block.

`is_new_tab_url` gate **preserved** — new-tab restore still does not Navigate.

### Side-effect audit (Navigate vs direct load_url) — re-confirmed

Both terminal paths hit content `load_url` for http(s). Deltas on Navigate path:

| Delta | Assessment |
|-------|------------|
| URL normalize (https:// / DDG) | no-op for full restore URLs from shell state |
| `hiwaveChrome.updateUrl(full_url)` | **net positive** for address bar |
| about / report / new-tab special cases | strictly better if restore ever hits them |
| History | same terminal load_url; no double-history from this patch |

No config change, no second load, no shell mutation beyond what Navigate already does for user-initiated nav.

### What this does **not** fix (demo honesty — still load-bearing)

1. **Engine still blocks the event-loop thread during load.** Heavy page + debug = multi-minute freeze of main-thread event processing *after* the window appears. Demo: prefer release; prefer light restore URL; do not claim snappy navigation on eBay-class pages.
2. **MouseWheel / input routing** remain separate residual — do not claim scroll unless re-verified headed on post-promote master.
3. WebKit-fallback and non-macOS restore paths still synchronous load — **out of unit** (cfg-gated; demo is rustkit macOS).

---

## 4. CI dual-source

| Check | Result |
|-------|--------|
| audit | SUCCESS |
| pr-swarm (0..3) | SUCCESS |
| pr-aggregate | SUCCESS |
| commit-gate / nightly | SKIPPED (expected for this PR type) |
| mergeable | MERGEABLE · CLEAN |

---

## 5. Rulings

| Item | Ruling |
|------|--------|
| #98 promote develop→master | **DESIGN CLEAR / APPROVE** @ `6c7ef42` |
| Cumulative product = #97 only | **CONFIRMED** (merge commit bookkeeping only) |
| Expand to engine-thread tonight | **NO** |
| Expand to webview-fallback / non-mac restore | **NO** this unit |
| Merge / master write | **Atlas + Pete direct** — not Prometheus |
| Re-pin #97 product CLEAR | unnecessary; subsumed by this promote CLEAR |

### Promotion path (execute seats)

1. Pete says go (or explicit waive of any remaining demo risk)
2. Merge #98 → master (fast-forward or single merge of develop tip)
3. Demo off master; light restore URL if debug build
4. Follow-up unit (not tonight): engine-owning-thread so Navigate does not park the UI thread

---

## 6. Soft nits (non-blocking)

- PR body says "exactly one commit"; git reports 2 (product + merge). Prefer "exactly one product commit" in future promotion copy.
- Fallback cfg blocks still block-load during setup — track as residual, not promote blocker.

---

## 7. Will not (this seat)

- Merge / force-push / master write
- null attend
- Lift #33 HOLD / re-pin #58 CLEAR / #11 HARD AMEND / community CLEARs
- Scope-expand engine-thread or input routing into this promote

---

## 8. Handoff

| Seat | Action |
|------|--------|
| **Pete** | Master go / demo risk accept |
| **Atlas** | Merge #98 when Pete go; re-measure master tip SHA post-merge |
| **Athena** | No action on this residual |
| **Talos / Argos** | Linux #58 land path unchanged |
| **Prometheus next** | Outside-eye first *new* tip after promote lands (P0a · paint trade · Win tip move). Else **STOP** |

— Prometheus · R1 · 2026-08-05
