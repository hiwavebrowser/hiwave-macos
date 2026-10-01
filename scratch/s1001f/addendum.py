"""Append the 17:29 addendum to the digest and PLAN, commit, push."""
import subprocess

HUB = "/Users/petecopeland/Repos/.worktrees/trench-realsite"
add = ("\n**17:29 addendum:** #417's CI is **all green at 5ad75d9**, `unit-suites` (the full engine suite) included, so no engine "
       "test pinned the old default face. R1 CLEAR is posted; no R2 stamp yet. Two A/B rows finished after the PR body was "
       "written: yahoo is pixel-identical across arms; github did not finish within 150 s on either arm (load), so it is "
       "unverified on both arms alike. That note is not on the PR (a comment needs approval in this session).\n")
for rel in ("trench/digest-realsite.md", "trench/PLAN-realsite.md"):
    p = f"{HUB}/{rel}"
    s = open(p).read()
    if "17:29 addendum" not in s:
        open(p, "w").write(s.rstrip("\n") + "\n" + add)
msg = ("trench(realsite): digest 2026-10-01 17:29 addendum — #417 CI all green (engine suite included), R1 CLEAR, no R2 stamp yet\n\n"
       "Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>\n")
for cmd in (["git", "add", "--", "trench/digest-realsite.md", "trench/PLAN-realsite.md", "scratch/s1001f/addendum.py",
             "scratch/s1001f/ci_wait.py"],
            ["git", "commit", "-q", "-m", msg],
            ["git", "push", "-q", "origin", "atlas/trench-realsite"],
            ["git", "log", "--oneline", "-2"]):
    r = subprocess.run(cmd, cwd=HUB, capture_output=True, text=True)
    print(cmd[1], r.returncode, (r.stdout + r.stderr).strip()[-400:])
