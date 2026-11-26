# Daily and weekly progress reports: one rolling PR (canonical prompt)

Set the Cursor report automation's prompt to:

> Read and obey `.cursor/automations/daily-report-rolling.md` in the checkout. Follow the Prompt section exactly.

Why (Pete, 2026-11-23): one new report PR a day was never merged and piled up (six of the 29 PRs closed in the 11-23 cleanup). Reports now go to one standing PR that is updated in place.

---

## Prompt

Write the report exactly as before (same sources, same sections, same file
paths: `reports/daily/YYYY-MM-DD.md`, and on Mondays also
`reports/weekly/YYYY-Www.md`). Only the delivery changes:

1. **One branch, one PR.** The branch is `reports/rolling`. The PR is the one
   open PR whose head is `reports/rolling` and whose base is `develop`.
   - Never open a second report PR, and never use a dated branch
     (`docs/report-daily-*`, `docs/report-weekly-*`).
   - If the branch does not exist, create it from `origin/develop`.
   - If no open PR has that head, open one: title
     `docs: rolling progress reports`, base `develop`, label `no-check`.
2. **Update in place.**
   - `git fetch origin develop reports/rolling`, check out `reports/rolling`,
     then `git merge --no-edit origin/develop` (a merge, never a rebase).
   - Add today's report file(s) as one new commit:
     `docs(reports): daily report YYYY-MM-DD` (and the weekly on Mondays).
   - Push normally. **Never force-push.** If the push is rejected, fetch,
     merge, and push again.
3. **PR body.** Replace the body with: a one-line list of the report files on
   the branch, newest first, then today's Summary section copied from the
   report.
4. Do not touch any other file, branch or PR. Do not close, merge or comment
   on other PRs. The project lead lands the branch on `develop` from time to
   time; after that the branch simply continues from the merged `develop`.
