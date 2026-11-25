# Coverage pins: one rolling PR (canonical prompt)

Set the Cursor coverage automation's prompt to:

> Read and obey `.cursor/automations/coverage-rolling.md` in the checkout. Follow the Prompt section exactly, then do the day's coverage task as before.

Why (Pete, 2026-11-23): one `coverage/auto-develop-*` PR a day was never batched (nine of the 29 PRs closed in the 11-23 cleanup). Coverage pins now go to one rolling PR that the project lead batches like any other.

---

## Prompt

Choose and write the day's coverage tests exactly as before (portable pins for
risky behaviour that only a macOS or headless test covers, with the same PR-body
sections: "Risky behavior now covered", "Test files added/updated", "Why these
tests materially reduce regression risk", "Validation"). Only the delivery
changes:

1. **One branch, one PR.** The branch is `coverage/rolling`; the PR is the one
   open PR with that head and base `develop`.
   - Never open a second coverage PR or a dated `coverage/auto-develop-*` branch.
   - If the branch does not exist, create it from `origin/develop`.
   - If no open PR has that head, open one: title `test: rolling portable
     coverage pins`, base `develop`, **no** `no-check` label (it is reviewed
     and batched).
2. **Update in place.**
   - `git fetch origin develop coverage/rolling`, check out `coverage/rolling`,
     `git merge --no-edit origin/develop` (never a rebase).
   - Skip a pin the branch or `develop` already has.
   - Add the day's pins as one commit: `test: portable pins YYYY-MM-DD (<rows>)`.
   - Run `cargo test -p <crate> --lib <filter>` for each new pin and put the
     result in the PR body. Push normally; **never force-push**.
3. **PR body.** Keep one section per day, newest first, with the four headings
   above. When the branch has been landed (its commits are on `develop`),
   drop that day's section.
4. **Size cap.** If the branch already has 5 unlanded days of pins, do not
   add more; write "waiting for a batch" at the top of the body and stop.
5. Do not touch any other branch or PR.
