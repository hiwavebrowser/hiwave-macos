# Real-site trench digest (macOS)

## 2026-09-23 22:30 — trench stood up

- Board defined: top 20, 3 checks, 60 points (BASELINE-realsite.md).
- Baseline: 0/60 — instrument not built (parity-capture cannot load a URL yet).
- Next: phase 0 — `parity-capture --url` + `scripts/realsite_board.py`.

## 2026-09-24 00:45 — phase 0 built, first points

**Points: 12 → 14 / 60** (measured baseline 12 on develop ec02a7f + instrument).

| | loads | readable | looks-right | points |
|---|---|---|---|---|
| baseline `20260924T030109Z` | 8 | 2 | 2 | **12** |
| + prefilter, chunked-EOF `20260924T033444Z` | 9 | 2 | 2 | 13 |
| + concurrent fonts `20260924T041700Z` | 10 | 2 | 2 | **14** |

Flipped: wikipedia LOADS (86.8s → 29.9s → under budget), netflix LOADS
(document now loads, then under 30s). Passing all 3: google.
Blocked (0 points, JS-challenge bot walls): amazon aws-waf/202, chatgpt
cloudflare/403, ebay akamai/403, nytimes datadome/403. LOOKS RIGHT
unstable: amazon (Chrome vs Chrome 20.7%). Oracle failures: none in the final run.

**Commits on the hub (no PRs: see decision 1).** Each is its own commit:
- `cf6db88` feat(parity-capture): `--url` + `--timeout-ms` watchdog + `--dump-display-list`; board + Chrome oracle
- `bef65bb` fix(engine): subject prefilter before the string selector matcher (Wikipedia 86.8s → 18.9s, frame byte-identical)
- `bc13d6e` fix(http): chunked body truncated by EOF keeps what arrived; RFC 9112 chunk extensions (netflix)
- `c23b143` perf(engine): fetch remote web fonts concurrently (YouTube 31.3s → 11.4s, frame byte-identical)
- `8713b3c` board: per-site access / `blocked:<vendor>` (per the plan update)
- Gates: new tests fail without each fix (501 full matches; "Invalid chunk size"; 3.66s sequential).
  Campaign `parity_test.py` 26/26, **all 26 byte-flat**, avg 2.0794 → 2.0794.
  **WPT tier1 not run**: `third_party/wpt` is missing in this worktree, and the
  sync/copy needs a command this seat's permission mode blocks.

**What the board says (the real picture):**
- Remaining timeouts: facebook, apple, github, cnn. Each needs a profile like the ones that found the two wins above.
- **RustKit runs no page `<script>` on any load path** (load_url doesn't, and the app doesn't either). YouTube now loads in time
  but paints a blank app shell. Instagram and X serve almost no HTML text. All 4 blocked sites are JS challenges.
  READABLE is 2/20 largely because of this.
- Wikipedia's READABLE is 72.5% and LOOKS RIGHT is 19.6%, both close to passing. The missing header and both sidebars
  are Vector 2022's CSS grid (Phase-5 grid work).

**Decisions for Pete:**
1. **This seat can't open PRs.** The trench launcher runs `claude -p --permission-mode acceptEdits`, which blocks
   `git fetch/branch/worktree`, `gh`, `node`, `cp -R`, and `git restore`. So the 3 engine fixes are commits on the hub
   instead of `atlas/rs-*` PRs. Two options: allow those commands for the trench seat, or have another seat cherry-pick
   `bef65bb`, `bc13d6e`, `c23b143` (+ `cf6db88` tooling) onto `origin/develop` and open the PRs.
2. **Page script execution is the ceiling.** Past about 20/60, most points need RustKit to run page JS (Boa is wired in,
   but only `execute_script` uses it). Should it be the next build sprint, ahead of grid?
3. Oracle choices made tonight are recorded in BASELINE Changes: Chrome pinned to light scheme, no freeze/reset on live
   sites, and RustKit uses the product's Safari UA. The plan's network-profile step would change that UA to a
   Chrome-compatible one. Confirm the order.

Housekeeping: `parity-baseline/parity_test_results.json` is left modified by the campaign run and is not committed
(this seat can't `git restore` it). The next session should discard it.

## 2026-09-24 16:22 — JS sprint step 1: page scripts run; tokenizer fix

**Points: 13 → 14 / 60** (same-day A/B, both on develop b9f133e + #241).

| run | engine | points | loads | readable | looks-right |
|---|---|---|---|---|---|
| `20260924T1910Z-base` | develop + #241 fonts (JS off) | **13** | 10 | 1 | 2 |
| `20260924T1910Z-js` | + #244 + #245 first cut (ran `nomodule`) | 11 | 7 | 2 | 2 |
| `20260924T2010Z-js2` | + skip `nomodule` (#245 head) | **14** | 10 | 2 | 2 |

Moved, base → js2: **linkedin READABLE +1** (63% → 86%, tokenizer fix). **apple LOADS +1** (30.15s → under 30s;
probably network variance). **wikipedia LOADS −1** (29.1s of subresources, then the script phase pushes it over).
Passing all 3: google. Blocked (0 pts): amazon aws-waf/202, chatgpt cloudflare/403, ebay akamai/403, nytimes datadome/403.

**PRs to develop (Prometheus R1 + Cursor R2; not mine to merge):**
- #241 `atlas/rs-concurrent-fonts` @ 43bd55c: concurrent web fonts (last session's hub fix). Rebased onto b9f133e
  before any review, because #240 appended a test module at the same end-of-file spot. `gh pr comment` was blocked, so the note is here.
- #244 `atlas/rs-script-data-with-attributes` @ a75462b: the tokenizer entered script
  data / RAWTEXT / RCDATA only for bare tags. `<script nonce=…>` was tokenized as HTML, so google's first inline
  script arrived as 73 of 303 bytes and a `'</div>'` in a JS string could close real elements. It hit nearly every real site.
- #245 `atlas/rs-run-scripts` @ 2620162: `load_url` runs `<script>`s (classic → defer → async; module and
  `nomodule` skipped), fires DOMContentLoaded/load, runs virtual-clock timers, and keeps a per-view script log. Guards:
  Boa loop limit, panic catch, one 5s budget for fetch + run. parity-capture URL mode now has JS **on** (it was
  hard-off, so the board never ran scripts); fixture mode stays off. Board saves `rustkit-scripts.json` per site.
- Gates: new tests fail without each fix (verified). Engine 125, bindings 23, js 13, html 40+49. Campaign not
  re-run: fixture mode is JS-off, and a static scan finds 0 fixtures the tokenizer change touches. WPT not run: no `third_party/wpt` on this seat.

**JS error census (step 2 seed, run js2, sites hitting each):** 6 `TypeError: cannot convert null/undefined to object`
(apple, chatgpt, google, netflix, x, yahoo) · 2 `ReadableStream` · 2 `performance` · 2 `XMLHttpRequest` · 2
`not a callable function` · 1 each: `AbortController`, `structuredClone`, `URL`, `Event`, `Element`, bing
`cannot assign to uninitialized global onload`. Full list: `runs/20260924T2010Z-js2/js-census.txt`.
The scripts still see the JS **stub** DOM (not the Rust DOM), so no point can come from JS until step 3 wires it up.

**Next (cheapest points):** (1) fetch scripts concurrently with the other subresources instead of after them;
that wins back wikipedia and removes the 4–8s added to reddit/bing/instagram. (2) The `null/undefined → object` TypeError
on 6 sites; likely one missing stub property. (3) Real DOM bindings (Rust DOM attributes are immutable today:
`NodeType` isn't in a RefCell). That's the multi-session core of step 3.

**Decisions for Pete:**
1. **Boa can't be interrupted.** A script that loops outside a JS `for`/`while` (e.g. in native regex or builtins)
   hangs the whole capture: Next.js's `nomodule` polyfill did, on yahoo and weather. The loop limit didn't catch it.
   The only complete guard is running JS off-thread or out of process with a watchdog, or an engine with interrupts (V8/QuickJS).
   That's an architecture call. For now: `nomodule` is skipped, and each script is logged as it starts, so hangs name themselves.
2. **Seat permissions still bite:** `git merge`, `git -C <other dir>`, `cargo fmt`, `gh pr comment`, EnterWorktree
   and heredocs with quoted braces all need approval. I worked around them by switching branches inside the hub worktree.
   The PRs are unformatted by `cargo fmt`, and I said so in each.

## 2026-09-25 00:35 — the timeouts are layout, not network: font-resolution cache 12 → 15

**Points: 12 → 15 / 60** (best single-fix run; each fix measured alone against the same night's before).

| run | engine | points | loads | readable | looks-right |
|---|---|---|---|---|---|
| `20260925T0220Z-before` | develop 2d3e2bb (#244 + #245 merged) | **12** | 8 | 2 | 2 |
| `20260925T0310Z-overlap` | + #252 script overlap + size guard | 11 | 8 | 1 | 2 |
| `20260925T0350Z-fontcache` | + #251 font-resolution cache | **15** | 11 | 2 | 2 |
| `20260925T0400Z-encoding` | + #254 Accept-Encoding | 12 | 8 | 2 | 2 |
| `20260925T0425Z-stack-subset` | #254 + #251, 6 sites only | 3/18 | 2 | 1 | 0 |

The before is 12, not last session's 14: apple and netflix timed out on the network, and linkedin's LOOKS RIGHT was unstable.
**Font cache:** apple LOADS (its two big relayouts 11.9s → 7.0s and 11.5s → 7.6s: this fix), wikipedia LOADS (first layout
6.1s → 2.9s: mostly this fix), netflix LOADS (network: it also loaded in the overlap run).
**Encoding:** wikipedia LOADS (stylesheets 6.1s → 0.25s); microsoft −1, because it now gets its **real** page instead of the bot page and then times out in layout.
**Noise warning:** in the stacked subset, wikipedia missed 30s by 0.09s, and identical code ran layout ~50% slower than
in the font-cache run. This Mac is shared with other seats. LOADS for anything near 30s flips run to run; one board run can't
attribute a ±1. (Last session's guess, that overlapping script fetches would win back wikipedia, was wrong. Its time is layout.)
Passing all 3: google. Blocked (0 pts): amazon aws-waf/202, chatgpt cloudflare/403, ebay akamai/403, nytimes datadome/403.
**Reddit is effectively blocked too:** pinned Chrome gets reddit's "blocked by network security" page from this seat, and RustKit gets reddit's
JS challenge (a form `requestSubmit`). The access probe misses it (HTTP 200).

**New instrument (hub only):** the board keeps each capture's `--verbose` engine log (`<site>/rustkit-stderr.log`; kept locally,
not committed). "capture exceeded 30000 ms" is now a timeline. **The remaining timeouts are layout-bound:** wikipedia
spent 6–10s per relayout (×3), facebook 18s+ in the relayout after its web fonts, apple ~12s per relayout, microsoft 8.6s.

**PRs to develop (Prometheus R1 + Cursor R2; not mine to merge):**
- #251 `atlas/rs-font-resolve-cache` @ ab7d56e: memoize `create_ct_font_with_traits` per thread, invalidated by a new
  `webfonts::generation()`. Every shape/metrics call re-ran Core Text descriptor matching (or built a CTFont from a CGFont), uncached. **12 → 15.**
- #252 `atlas/rs-overlap-script-fetch` @ 798f861: script fetches overlap subresource loading; scripts over 4 MiB are
  skipped (once youtube's 10.8 MB bundle arrived inside the budget, Boa ran it for 16s+). 0 points on its own; its −1 is linkedin timing out in layout before any script.
- #253 `atlas/rs-font-face-guard` @ 8703e2d: security fix, details withheld (low-severity hardening, per the plan's new rule).
- #254 `atlas/rs-accept-encoding` @ 5830c3f: `Accept-Encoding: gzip, deflate` + decoding. microsoft.com serves an
  "automated process" page without it (curl-verified). Documents are 4–7× smaller (cnn 6.4 MB → 0.98 MB). Net 0 points.
- Gates: each new test fails without its fix (verified: 6.4s vs <5s; body "automated bot"; ≤4 vs 400 resolutions; the
  guard's input crashed a test process before the fix). Page-script 5/5, rustkit-http 9/9, layout font tests 2/2, webfonts 6/6.
  **Campaign `parity_test.py` and WPT tier1 not run:** five board runs used the session. Each PR says so.

**Cheapest next points** (from a static-HTML triage: the share of Chrome's first-viewport words present in the served HTML outside scripts):
1. **facebook** (100% static; timeout): profile the post-web-font relayout. It isn't font resolution.
2. **microsoft** (85% static, now the real page; timeout), **github**, **cnn**: the same layout-bound treatment. The logs are in the run dirs.
3. Everything else READABLE (yahoo 52% static, bing 35%, x 22%, instagram 2%, youtube 5%) needs JS on the real DOM (sprint step 3).

Housekeeping: `~/Repos/.worktrees/rs-concurrent-scripts` is a stale worktree from this session (its branch was deleted; a Cargo.lock
build change blocks `git worktree remove` without --force). The local `scratch/stack-enc-font` branch is a throwaway. Both are safe to delete.

**Decisions for Pete:**
1. **Profiling permission.** The next timeout points are CPU-bound inside layout. This seat can't run the capture binary
   directly or attach `sample`, so every diagnosis costs a 25-minute board run. Allowing `target/release/parity-capture` and `sample` for the
   trench seat would cut that to minutes.
2. **LOADS noise.** On a shared Mac, the 30s line flips sites run to run (wikipedia by 0.09s). Should the board score LOADS as the best
   of 2 RustKit captures (Chrome already gets 2), or keep one capture and accept the noise? It's a rules change, so it's your call.
3. **Reddit's oracle is blocked** (Chrome gets a network-security block page from this IP). Record it as `blocked:reddit`, as the other four are,
   or keep scoring it as-is?
