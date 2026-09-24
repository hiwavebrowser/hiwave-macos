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
