"""Append this session's digest section and PLAN note (once), commit the session's hub artifacts and push
atlas/trench-realsite. usage: hubcommit.py <commit-message-file>"""
import glob, subprocess, sys

HUB = "/Users/petecopeland/Repos/.worktrees/trench-realsite"
S = f"{HUB}/scratch/s1001h"
msg = sys.argv[1]
for target, src, marker in ((f"{HUB}/trench/digest-realsite.md", f"{S}/digest.md", "## 2026-10-01 23:10"),
                            (f"{HUB}/trench/PLAN-realsite.md", f"{S}/plan-note.md", "**2026-10-01 23:10:")):
    cur = open(target).read()
    if marker in cur:
        print("already appended:", target)
        continue
    add = open(src).read()
    open(target, "w").write(cur.rstrip("\n") + "\n" + add)
    print("appended", len(add), "chars to", target)
paths = ["trench/digest-realsite.md", "trench/PLAN-realsite.md"]
for pat in ("scratch/s1001h/*.py", "scratch/s1001h/*.md", "scratch/s1001h/*.txt",
            "scratch/s1001h/l0/*.py", "scratch/s1001h/l0/*.html", "scratch/s1001h/l0/*.chrome.json",
            "scratch/s1001h/l0/chrome.json",
            "scratch/s0929e/camp-dev-f657cf2/*.json", "scratch/s0929e/camp-dev-f657cf2/ratchet.txt",
            "scratch/s0929e/camp-ls-848c872/*.json", "scratch/s0929e/camp-ls-848c872/ratchet.txt",
            "scratch/s0929e/camp-gf-026fcc5/*.json", "scratch/s0929e/camp-gf-026fcc5/ratchet.txt"):
    paths += [q[len(HUB) + 1:] for q in glob.glob(f"{HUB}/{pat}")]
for cmd in (["git", "add", "--"] + paths,
            ["git", "commit", "-q", "-F", msg],
            ["git", "push", "-q", "origin", "atlas/trench-realsite"],
            ["git", "log", "--oneline", "-1"],
            ["git", "status", "-sb", "--untracked-files=no"]):
    r = subprocess.run(cmd, cwd=HUB, capture_output=True, text=True)
    print(cmd[1], r.returncode, (r.stdout + r.stderr).strip()[-300:])
