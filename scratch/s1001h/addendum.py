"""Append the 23:14 addendum to the digest, commit and push the hub branch."""
import subprocess
HUB = "/Users/petecopeland/Repos/.worktrees/trench-realsite"
p = f"{HUB}/trench/digest-realsite.md"
add = ("\n**23:14 addendum:** #426's CI is **all green at 026fcc5** (`unit-suites`, the full engine suite, included) "
       "and R1 CLEAR is posted. No R2 stamp yet; not merged.\n")
cur = open(p).read()
if "23:14 addendum" not in cur:
    open(p, "w").write(cur.rstrip("\n") + "\n" + add)
msg = ("trench(realsite): digest 2026-10-01 23:14 addendum — #426 CI all green (engine suite included), R1 CLEAR, "
       "no R2 stamp yet\n\nCo-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>\n")
for cmd in (["git", "add", "--", "trench/digest-realsite.md", "scratch/s1001h/addendum.py"],
            ["git", "commit", "-q", "-m", msg],
            ["git", "push", "-q", "origin", "atlas/trench-realsite"],
            ["git", "log", "--oneline", "-1"],
            ["git", "status", "-sb", "--untracked-files=no"]):
    r = subprocess.run(cmd, cwd=HUB, capture_output=True, text=True)
    print(cmd[1], r.returncode, (r.stdout + r.stderr).strip()[-200:])
