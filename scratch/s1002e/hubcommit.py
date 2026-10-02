"""Append this session's digest section and PLAN note (once), commit the session's hub artifacts and push
atlas/trench-realsite. usage: hubcommit.py <commit-message-file>"""
import glob, subprocess, sys

HUB = "/Users/petecopeland/Repos/.worktrees/trench-realsite"
S = f"{HUB}/scratch/s1002e"
msg = sys.argv[1]
for target, add, marker in ((f"{HUB}/trench/digest-realsite.md", open(f"{S}/digest.md").read(), "## 2026-10-02 14:15"),
                            (f"{HUB}/trench/PLAN-realsite.md", open(f"{S}/plan-note.md").read(), "**2026-10-02 14:15:")):
    cur = open(target).read()
    if marker in cur:
        print("already appended:", target)
        continue
    open(target, "w").write(cur.rstrip("\n") + "\n" + add)
    print("appended", len(add), "chars to", target)
paths = ["trench/digest-realsite.md", "trench/PLAN-realsite.md", "scratch/s1002e_test.rs"]
for pat in ("scratch/s1002e/*.py", "scratch/s1002e/*.md", "scratch/s1002e/*.txt", "scratch/s1002e/*.png",
            "scratch/s1002e/ext/index.html", "scratch/s1002e/ext/css/*.css", "scratch/s1002e/ext/img/*.png"):
    paths += [p[len(HUB) + 1:] for p in glob.glob(f"{HUB}/{pat}")]
subprocess.run(["git", "add", "-f"] + paths, cwd=HUB, check=True)
subprocess.run(["git", "commit", "-q", "-F", msg], cwd=HUB, check=True)
print(subprocess.run(["git", "log", "--oneline", "-1"], cwd=HUB, capture_output=True, text=True).stdout)
r = subprocess.run(["git", "push", "origin", "atlas/trench-realsite"], cwd=HUB, capture_output=True, text=True)
print((r.stdout + r.stderr)[-400:])
