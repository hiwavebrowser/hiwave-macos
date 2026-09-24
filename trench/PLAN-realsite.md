# Real-site trench — plan and session rules

Read `trench/BASELINE-realsite.md` first. One metric: realsite points out of 60.

## Where things live

| What | Where |
|---|---|
| Board, runner, results, digest | hub branch `atlas/trench-realsite` (worktree `~/Repos/.worktrees/trench-realsite`). Pushed, never merged, never force-pushed. |
| Engine fixes | one branch per fix, `atlas/rs-<slug>` from `origin/develop`, PR to `develop`. |
| Pinned Chrome | `PARITY_CHROME_PATH=~/Repos/hiwave/hiwave-macos/.browsers/chrome/mac_arm-148.0.7778.216/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing`. Playwright's default here is 143; never capture the oracle with it. |

## Phase 0 — build the instrument (first sessions)

Land these as PRs to `develop`, since they are tooling the whole repo can use:

1. **`parity-capture --url <url>`** (new flag; `--html-file` stays). Route it
   through `Engine::load_url`, then `load_subresources`, then run scripts and
   the event loop for a bounded settle time (`--settle-ms`, default 5000).
   Then dump the frame and layout exactly as `--html-file` does. Hard 30s
   timeout. Exit non-zero on crash or timeout, and print the reason.
2. **`scripts/realsite_board.py`**: for each site in `websuite/realsite-top20.json`:
   - Chrome 148 twice (for self-noise), using the oracle's deterministic launch
     options. Capture a screenshot and the first-viewport visible text.
   - RustKit once, via `parity-capture --url`, keeping its frame and layout
     text runs.
   - Score LOADS / READABLE / LOOKS RIGHT exactly as BASELINE defines them.
   - Write `trench/realsite/runs/<ts>/<site>.json` plus `summary.json`, and
     print a one-screen table. Exit 0 even when sites fail; exit non-zero only
     if the instrument itself broke.

Commit the first real board run on the hub branch and replace the `0 / 60`
placeholder in BASELINE with the measured number. Then go to phase 1.

## Access blocks (bot protection) — record them, fix the shared cause

Measured 2026-09-23 with curl, logged out, from this Mac. Four of the 20 are
behind JavaScript-challenge bot managers. Headers and HTTP/2 alone do not
flip any of them:

| Site | Status | Vendor |
|---|---|---|
| chatgpt | 403 | Cloudflare (`__cf_bm`, challenge) |
| ebay | 403 | Akamai Bot Manager (`bm_s`) |
| nytimes | 403 | DataDome (`x-datadome`, captcha) |
| amazon | 202 | AWS WAF (`x-amzn-waf-action: challenge`) |

- **The board must record access per site:** the HTTP status of the top-level
  document, the vendor (from `server` / `x-datadome` / `x-amzn-waf-action` /
  `cf-ray` / `bm_*` cookies), and whether a challenge page was served. A
  blocked site scores LOADS=0 with reason `blocked:<vendor>`, never just
  "failed". Report blocked sites as their own line in the digest.
- **Shared fix, in this order (each its own `rs-` PR with a test):**
  1. Network profile: default UA
     `Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36 HiWave/<version>`
     (Chrome-compatible with a HiWave token, as Edge and Brave do; currently
     `RustKit/1.0` in rustkit-http and rustkit-net). Send standard navigation
     headers (Accept, Accept-Language, Accept-Encoding with decoding,
     Sec-Fetch-*, Upgrade-Insecure-Requests), keep-alive, and HTTP/2 via ALPN.
  2. Challenge completion: a persistent cookie jar across navigations,
     `Set-Cookie` then reload/redirect, `location.reload()`, timers,
     fetch/XHR, `crypto.subtle`, and consistent `navigator` / `screen` /
     `Intl` values. The four sites above are the acceptance test.
- **No evasion.** Do not forge fingerprints, solve captchas, rotate IPs, or
  impersonate a different browser's TLS stack. The goal is to be a complete
  browser that runs the challenge honestly. If a vendor still blocks after
  that, record it and move on.

## Phase 1 direction (Pete, 2026-09-24): JavaScript before CSS grid

RustKit runs no page `<script>` on any load path, and that caps READABLE and
LOADS on most of the board (JS-rendered apps, and the four JS-challenge walls).
Grid layout waits. The JS sprint, in order, each step its own `rs-` PR with a
test that fails without it:

1. **Run scripts on the load path.** `load_url`: inline and external `<script>`
   in document order (classic scripts block; `defer` after parse; `async`
   when fetched; `type=module` is recorded as unsupported for now), through
   `rustkit-js` (Boa, feature already in the tree) and `rustkit-bindings`.
   Then dispatch `DOMContentLoaded` / `load`, run timers and microtasks inside
   `--settle-ms`, and re-style and re-layout after DOM mutation. Enforce a
   per-script time budget: a hung script must never hang the capture.
2. **A JS error census on the board.** Record per site the uncaught
   exceptions and missing-API accesses (`X is not a function`,
   `undefined property Y`). Rank the missing APIs by how many sites hit them,
   and put the top 10 in each digest.
3. **Implement APIs in census order.** Only what sites actually hit, no
   speculative DOM breadth. Expect `document.createElement` / `append*` /
   `innerHTML`, `querySelector*`, `classList`, `addEventListener`,
   `setTimeout`, `requestAnimationFrame`, `fetch` / XHR, `location`,
   `history`, `localStorage`, cookies, `navigator` / `screen` / `matchMedia`.
4. **The challenge walls** (amazon, chatgpt, ebay, nytimes) are the
   acceptance test for cookies + reload + timers + `crypto.subtle`, under the
   no-evasion rule above.

**Report, don't decide:** Boa's speed on multi-megabyte production bundles
(YouTube, Facebook). If a site's scripts cannot finish inside the 30s LOADS
budget after the easy wins, measure it (script bytes, Boa time) and put it
under decisions for Pete. Switching engines (V8 / QuickJS) is an
architecture call, not a trench call.

**Security:** script execution is the largest attack surface this browser has.
Keep the sandbox (no filesystem, no process access from JS), enforce
same-origin on fetch/XHR and cookies from day one, and follow the
security-findings-stay-private rule for anything found.

## Phase 1 — grind the points

Each session:

1. Run the board. Record the before number.
2. Pick the **cheapest next point**: the site/check where one engine fix is
   likely to flip it, or where one fix flips several sites. Say why in one line.
3. Find the root cause in RustKit (Aleph first), fix it on `atlas/rs-<slug>`
   with a regression test that fails without the fix, and open a PR to `develop`.
4. Re-run the board with the fix built. Put the before → after number in the
   PR body with the per-site rows that moved. Do not commit
   `parity-baseline/*` receipts in `rs-` PRs; put the campaign numbers in the
   body. (Receipts in every PR are why every merge forces a restack.)
5. Also run `python3 scripts/parity_test.py` and `python3 scripts/wpt_tier1.py`.
   A fix that makes any campaign case worse, or drops WPT below 24/26, needs
   an explicit line in the PR body. Never hide it.

## Review and merge (not yours)

PRs are reviewed by Prometheus (R1 design, Grok) and the Cursor R2 gate bot.
Prometheus merges when R1 CLEAR + `R2-STAMP: PASS @ <sha>` + green + CLEAN at
the same head SHA. **Never merge your own PR. Never force-push a branch
someone has reviewed** without saying so in the PR.

## Session rules

- Aleph before grep/read.
- `claude -p` headless: run cargo builds in the FOREGROUND, and never end a
  turn waiting on a background task.
- Cap about 3h per session, and stop cleanly.
- Before stopping, append a `## <date> <HH:MM>` section to
  `trench/digest-realsite.md` on the hub branch. Include: points before → after,
  per-check totals, PRs opened (numbers + SHAs), at most 3 decisions for Pete.
  Commit and push the hub branch.
- Do not ping anyone. The noon digest delivers.
- Never edit the site list or thresholds to move the number.
- **Security findings stay private.** hiwave-macos is public. If a site exposes a security defect (sandbox, cross-site cache or cookie leaks, TLS/cert handling, script isolation), do NOT describe the mechanism in a PR body, commit message, or digest. Write "security fix, details withheld" and add one line for Pete under decisions.
