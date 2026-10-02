# Trench baseline — real-site board (macOS)

end_date: 2026-10-23
exit_metric: realsite points >= 60/60 (all 20 sites pass all three checks)
max_open_prs: 8
pr_repo: hiwavebrowser/hiwave-macos
pr_head_prefix: atlas/rs-

**Started:** 2026-09-23 · **Authorized by:** Pete ("build the real-site board, top 100 first, lets get parity there, start with the top 20 and trench it and the trench goes around the clock")
**Hub branch:** `atlas/trench-realsite` (board, results, digest — never merged) · **Plan:** `trench/PLAN-realsite.md`

---

## The one metric

**Real-site points: 3 per site × 20 sites = 60.** The list is pinned in
`websuite/realsite-top20.json`.

```
BASELINE (2026-09-23 23:01 EDT):  12 / 60   loads 8 · readable 2 · looks-right 2
  run trench/realsite/runs/20260924T030109Z, engine = origin/develop ec02a7f
  (parity-capture --url on top), Chrome for Testing 148.0.7778.216.
  Oracle failed (0 points, reported): weather.
```

A site earns one point for each check it passes. All three are scored against
pinned Chrome for Testing 148.0.7778.216, loading the same URL in the same
board run, at 1280x800, logged out.

| Check | Passes when |
|---|---|
| **LOADS** | RustKit fetches the URL, runs the page, and produces a frame within 30s with no crash, and the frame is not blank (≥ 2% non-background pixels). |
| **READABLE** | ≥ 80% of the words Chrome shows in the first viewport appear in RustKit's first-viewport text runs (case-folded, de-duplicated). |
| **LOOKS RIGHT** | First-viewport pixel diff vs Chrome ≤ 15% (the campaign threshold). If Chrome-vs-Chrome for that site in the same run already exceeds 15% (rotating content), this check is scored `unstable`, counts 0, and is reported separately. It is never quietly dropped. |

### Why this metric and not the pixel campaign

The 26-case campaign averages about 2% on pages we wrote, and some fixes can no
longer move it (#212). Nobody switches browsers over fixture pages. They switch
when Google, YouTube, Amazon, Reddit and their bank *work*. This board counts
that directly, with Chrome as the oracle.

### Honesty rules

- Never edit the site list, thresholds, or viewport to make the number move.
  Changes need a dated line under **Changes** and restate the old number under
  the new rule.
- Live sites drift. Every board run captures Chrome fresh; keep the raw
  per-site JSON for each run under `trench/realsite/runs/<ts>/` on the hub
  branch.
- "Usable" (login, forms, video, scrolling) is **phase 2**, top 100. Phase 1
  does not claim it.

## Stop condition

Whichever comes first:

1. **60/60.** Then phase 2: top 100 plus the USABLE checks.
2. **Seven days with no point gained.** Write it plainly in the digest and
   propose a new angle or a funeral note.
3. `end_date` passes.

## Changes

- 2026-09-28 (Pete's OK): swapped out chatgpt (Cloudflare 403), ebay (Akamai 403) nytimes (DataDome 403) and amazon (AWS WAF 202, blank frame). All four refuse RustKit at the network layer, so they score 0 no matter what the engine does. Replacements from the wide board, chosen for structure, loading today, with a working Chrome oracle: walmart (product grids), shopify (marketing and commerce layout, 96% readable), squarespace (design-heavy editorial layout), lyft (marketing layout, 100% readable). The board stays at 20 sites / 60 points. Scores before and after this date are NOT comparable. The trend restarts here.

- 2026-09-23 — board defined. Baseline 0/60 pending instrument.
- 2026-09-23 — instrument built (`parity-capture --url`, `scripts/realsite_board.py`,
  `tools/parity_oracle/realsite.mjs`); measured baseline **12/60**. How the
  instrument reads the definitions above (none of these move a threshold):
  - *Blank*: < 2% of pixels differ (any channel > 8/255) from the frame's
    dominant colour.
  - *RustKit text runs*: display-list `text` ops whose box intersects the
    first viewport. *Chrome words*: visible text nodes (`checkVisibility`)
    with a client rect in the first viewport. Words = `\w+`, casefolded, deduped.
  - Chrome page with **no** first-viewport text: READABLE is `n/a`, 0 points,
    listed in `readable_unscored`.
  - *Oracle*: the first of the two Chrome captures that succeeded. If both
    fail (swiftshader screenshot timeouts on heavy pages), READABLE and
    LOOKS RIGHT count 0 and the site is listed in `oracle_failed`.
  - Chrome is pinned to light colour scheme and en-US (media emulation +
    `Sec-CH-Prefers-Color-Scheme: light`). Unpinned, it followed this seat's
    dark OS appearance on some launches (Google came back dark). The
    shakedown run 20260924T022252Z predates this pin; it scored 10/60 and is
    kept only as a record.
  - The oracle does **not** apply parity-freeze.js or the parity reset (those
    normalise fixtures; on a live site they change what the page does).
  - RustKit loads with the product's user agent (hiwave-app's Safari-like
    UA), so sites serve it what they serve HiWave users.
  - RustKit executes no page `<script>` on any load path today; the board
    measures that truthfully rather than simulating script execution.
- 2026-09-25 — scorer brought in line with PLAN "Access blocks": a site the
  access probe marks blocked scores LOADS = 0 (so READABLE / LOOKS RIGHT 0)
  even when RustKit paints something. Before, chatgpt scored 2 in
  `20260925T1925Z-subdl` for matching Cloudflare's interstitial, which Chrome got
  too. That record was rescored to 0 (marked `rescored`); no earlier run credited
  a blocked site. Old rule applied to that run: 17; new rule: 15.
- 2026-09-26 — A2 (PLAN, allowed tooling): a Chrome capture that lands on an
  error page (`final_url` is `chrome-error://`, or the top-level document is
  HTTP ≥ 400) is not a *successful* oracle capture. If both captures are like
  that, the existing oracle-failed rule applies: READABLE and LOOKS RIGHT count
  0, and the site is listed in `oracle_blocked`. The summary also reports
  `n/scorable` (points on sites where neither RustKit's access probe nor Chrome's
  oracle was blocked) next to `/60`, never instead of it. The live case was x:
  Chrome got "Access to x.com was denied" (HTTP 403), and RustKit's mostly white
  frame "looked right" against it. Run `20260926T0620Z-dev` (develop a0176dd):
  old rule 14, new rule **13** (x 2 → 1). Every other site scored the same as
  `20260926T0340Z-stack-logical`. reddit's oracle is also HTTP 403 now, but it
  already scored 0.
- 2026-09-26 — A6: each full run (including a chunked run summarised with
  `--summarize`) appends a row to `trench/realsite/trend.csv`.
- 2026-09-26 — A1 (Pete approved): the Chrome oracle runs **headed** (pinned CfT
  148, every deterministic flag kept, window off-screen at `-2400,0`, 1280x800).
  No spoofing and no `navigator.webdriver` change: Chrome identifying as itself.
  `realsite_board.py --oracle-headless` restores the old identity for
  comparisons. Same engine (develop 2b6a04e), same evening:
  - headless `20260927T0200Z-headless`: **19/60**, scorable 18/45. Oracle
    blocked: reddit (HTTP 403), x (HTTP error page), chatgpt, ebay, nytimes.
  - headed `20260927T0120Z-headed`: **18/60**, scorable 18/48. Oracle blocked:
    **none**. x and reddit now get the real site (HTTP 200), and so do chatgpt,
    ebay and nytimes (RustKit is still access-blocked on those three).
  - The one site that moved is facebook (LOOKS RIGHT 8.7% headless vs 18.3%
    headed). That is its usual oracle drift (Chrome-vs-Chrome 8–10% in both);
    no other check changed. The headed run overlapped a cargo release build
    for its first half, and 6/38 Chrome captures hit screenshot timeouts (x
    twice, github, yahoo, amazon, chatgpt), against 1/38 headless. That's
    contention, not the headed mode; the next headed runs will say.
  - Settled the same night: a clean headed run with nothing else on the
    machine, `20260927T0255Z-headed-dev` (develop 2ab8032), had 1/40 Chrome
    captures fail (weather), so the timeouts were contention. It scored
    **20/60**, scorable 20/48. x now scores against the real page (LOOKS
    RIGHT 13.4%, 1 → 2).


## Exit (2026-10-02): trench ended by Pete; thank you
Pete ended all trench runs on 2026-10-02 at 15:30 ET ahead of the next phase ("Z phase"), with thanks for the service. Final metric: real-site board 28/60 on develop e006c68 (13:00 quiet board), from 14/60 on 2026-09-25 and a 12/60 start on 2026-09-23. 64 digest sections, around 60 PRs landed on develop in the last week alone. Last structural finds, carried into the next phase: CSS background images had never painted on a live site (fetch landed as #443; painting is next), the image loader side door (closed by #438), the live-site JS gap map, and the instrument's limits (the 15% looks-right threshold, the 2% loads threshold, ±2 points of run noise). The hourly launchd job is unloaded (plist in ~/Library/LaunchAgents/paused/). The record stays on this branch; the frames are in hiwavebrowser/hiwave-renders-private.
