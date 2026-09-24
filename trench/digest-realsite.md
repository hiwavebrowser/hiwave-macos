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
