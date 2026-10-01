"""Commit this session's hub artifacts (digest and PLAN are already edited in place) and push
atlas/trench-realsite. usage: hubcommit.py <commit-message-file>"""
import glob, subprocess, sys

HUB = "/Users/petecopeland/Repos/.worktrees/trench-realsite"
msg = sys.argv[1]
paths = ["trench/digest-realsite.md", "trench/PLAN-realsite.md"]
for pat in ("scratch/s1001g/*.py", "scratch/s1001g/*.md", "scratch/s1001g/*.txt",
            "scratch/s0929e/camp-dev-ac1b067/*.json", "scratch/s0929e/camp-dev-ac1b067/ratchet.txt",
            "scratch/s0929e/camp-fallback-00e32a1/*.json", "scratch/s0929e/camp-fallback-00e32a1/ratchet.txt"):
    paths += [q[len(HUB) + 1:] for q in glob.glob(f"{HUB}/{pat}")]
for cmd in (["git", "add", "--"] + paths,
            ["git", "commit", "-q", "-F", msg],
            ["git", "push", "-q", "origin", "atlas/trench-realsite"],
            ["git", "log", "--oneline", "-1"],
            ["git", "status", "-sb", "--untracked-files=no"]):
    r = subprocess.run(cmd, cwd=HUB, capture_output=True, text=True)
    print(cmd[1], r.returncode, (r.stdout + r.stderr).strip()[-300:])
