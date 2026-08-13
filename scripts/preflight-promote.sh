#!/bin/bash
# preflight-promote.sh — refuse to open a promote PR that would eat `develop`.
#
# The landmine (hiwave-windows, 2026-08-06): a promote PR's HEAD branch IS the
# long-lived integration branch. With `delete_branch_on_merge=true` — correct
# hygiene for disposable feature branches — merging the promotion SILENTLY
# DELETES `develop`. Nothing in the merge output mentions it. Athena found out
# ~90 minutes later when an unrelated `gh pr create --base develop` failed with
# "Base ref must be a branch".
#
# No content is lost (master has everything), but every builder's next
# `--base develop` fails, and the reflex fix — pushing a STALE local develop —
# silently rolls the tree back. That second-order damage is the real hazard.
#
# Usage:
#   ./scripts/preflight-promote.sh              # check this repo, exit 1 if unsafe
#   ./scripts/preflight-promote.sh --post-merge # verify develop survived a promote
set -u

# --repo exists so this check can be RUN AGAINST A KNOWN-UNSAFE REPO as a
# positive control. A guard verified only where it passes is half a test
# (Athena, 2026-08-06): proving it goes green says nothing about whether it
# can still go red for the right reason.
REPO=""
MODE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --repo) REPO="$2"; shift 2 ;;
    *) MODE="$1"; shift ;;
  esac
done
[ -z "$REPO" ] && REPO=$(gh repo view --json nameWithOwner --jq .nameWithOwner 2>/dev/null)
if [ -z "$REPO" ]; then echo "preflight: not a gh-visible repo" >&2; exit 2; fi

FAIL=0

AUTO_DELETE=$(gh api "repos/$REPO" --jq .delete_branch_on_merge 2>/dev/null)
DEV_SHA=$(git ls-remote --heads "https://github.com/$REPO.git" develop 2>/dev/null | awk '{print substr($1,1,7)}')
MASTER_SHA=$(git ls-remote --heads "https://github.com/$REPO.git" master 2>/dev/null | awk '{print substr($1,1,7)}')
[ -z "$MASTER_SHA" ] && MASTER_SHA=$(git ls-remote --heads "https://github.com/$REPO.git" main 2>/dev/null | awk '{print substr($1,1,7)}')

echo "repo:                    $REPO"
echo "delete_branch_on_merge:  ${AUTO_DELETE:-unknown}"
echo "remote develop:          ${DEV_SHA:-MISSING}"
echo "remote master:           ${MASTER_SHA:-MISSING}"

if [ "$MODE" = "--post-merge" ]; then
  # After a promote: develop must still exist. Deliberately does NOT offer to
  # recreate it — that decision needs a human who has looked at ls-remote,
  # because auto-healing from a stale local tip is the rollback hazard.
  if [ -z "$DEV_SHA" ]; then
    echo
    echo "!! develop is GONE after the promote."
    echo "   Restore it from the REMOTE master tip (never from a local checkout):"
    echo "     git fetch origin && git push origin \$(git rev-parse origin/master):refs/heads/develop"
    exit 1
  fi
  echo
  echo "OK: develop survived the promote."
  exit 0
fi

if [ "$AUTO_DELETE" = "true" ]; then
  echo
  echo "!! UNSAFE: delete_branch_on_merge=true on $REPO."
  echo "   A develop->master promote PR will DELETE develop when merged."
  echo "   Fix (needs repo admin — builder tokens get 404 here):"
  echo "     gh api -X PATCH repos/$REPO -f delete_branch_on_merge=false"
  echo "   Or promote via a disposable head branch instead:"
  echo "     git push origin origin/develop:refs/heads/promote/\$(date +%Y-%m-%d)"
  echo "     gh pr create --base master --head promote/\$(date +%Y-%m-%d)"
  FAIL=1
fi

if [ -z "$DEV_SHA" ]; then
  echo
  echo "!! develop does not exist on origin — nothing to promote."
  FAIL=1
fi

[ "$FAIL" -eq 0 ] && echo && echo "OK: safe to open a develop->master promote PR."
exit "$FAIL"
