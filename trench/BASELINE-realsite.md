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
BASELINE (2026-09-23):  0 / 60   — instrument not built yet: parity-capture
                                   takes --html-file only and cannot load a URL.
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

- 2026-09-23 — board defined. Baseline 0/60 pending instrument.
