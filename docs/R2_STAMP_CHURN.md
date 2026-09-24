# R2 stamp always + stamp churn

Diagnosis from the 2026-09-24 Grok and GPT-terra runs of
[Hiwave_Macos PR reviewer](https://cursor.com/automations/1799b0cb-b119-11f1-a3d8-362438fd9788).

Canonical automation prompt: [`.cursor/automations/r2-gate-reviewer.md`](../.cursor/automations/r2-gate-reviewer.md).

## What "stamp always" fixes

Several GPT-terra runs reached a mechanical **PASS** in chat and never posted
`R2-STAMP` or labels:

| Run | PR | What happened |
|-----|-----|----------------|
| `bc-d40026ac…` (`2260`) | #244 | CI green, chat PASS — no comment tool call |
| `bc-568d33a7…` (`237e`) | #246 | Same |
| `bc-d63a3acf…` (`b583`) | #238 | Same |
| `bc-b367fe01…` (`1ce3`) | #239 | Same |

Root cause: after CI woke, agents treated generic "avoid messaging / external
destination" reminders as overriding the stamp contract, or simply forgot to
call the comment tool and treated the final message as the stamp.

**Fix:** hard rule that chat is not a stamp; must post GitHub comment + label
before ending. Prefer `gh pr comment` as fallback.

## Stamp churn root causes

### 1. FAIL while required checks are still pending

Example: `bc-7aaf6d51…` (`6713`) stamped **FAIL** on #241 / #242 while
`pr-aggregate` / `pr-swarm` were still `QUEUED`, citing gate 1 pending **and**
gate 2 DIRTY.

That creates noisy stamps and later flips when CI finishes (even if gate 2 still
fails, the stamp text and timing churn).

**Fix:** never stamp PASS or FAIL while required checks are pending. Wait until
terminal, then stamp once. Do not cite pending as a fail reason.

### 2. PASS → FAIL when `develop` moves (gate 2 DIRTY)

Grok-era runs (e.g. `32f1`) stamped PASS, then after another PR merged into
`develop`, remaining open PRs became `DIRTY` and were restamped FAIL without a
new push on those PRs.

Some of that is real gate-2 policy. Churn comes from **mass restamping unchanged
HEADs** on every automation tick.

**Fix:** skip a new comment when latest `R2-STAMP` already names current HEAD and
verdict+label are unchanged. Re-stamp only on HEAD move or verdict flip.

### 3. Conflicting scope line

Some runs appended "check both master and develop…" while the base rule is
develop-only. Agents inconsistently skipped or later reviewed retargeted PRs.

**Fix:** single scope rule — base `develop` only — in the canonical prompt.

### 4. Label API 404s

Removing missing `r2-pass` or adding a label that does not exist yet returned
404; agents sometimes treated that as "done".

**Fix:** create labels if missing; verify with `gh pr view --json labels`.

### 5. Same-SHA FAIL → PASS (usually legitimate)

Example: #235 gained `large-diff:` at the same HEAD and correctly flipped to
PASS. That is a verdict flip, not bugs — keep allowing restamp on verdict change.

## How to activate

1. Open the automation: https://cursor.com/automations/1799b0cb-b119-11f1-a3d8-362438fd9788
2. Replace the prompt with either:
   - the full **Prompt** section from `.cursor/automations/r2-gate-reviewer.md`, or
   - `Read and obey .cursor/automations/r2-gate-reviewer.md in the checkout. Follow the Prompt section exactly.`
3. Ensure the automation has PR comment / label tools (or `gh`) enabled.
4. Clear contradictory standing rules from automation `MEMORIES.md` if they conflict
   (especially "do not stamp" vs "stamp always").

## Success checks on the next few runs

- Every terminal review that claims PASS or FAIL has a matching `R2-STAMP` comment
  on the PR at that HEAD.
- No stamp while required checks are `QUEUED` / `IN_PROGRESS`.
- Unchanged FAIL HEADs are not comment-spammed on every tick.
- Chat-only PASS with null stamp no longer appears in transcripts.
