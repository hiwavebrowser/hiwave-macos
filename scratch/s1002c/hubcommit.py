"""Append this session's digest section and PLAN note (once), commit the session's hub artifacts and push
atlas/trench-realsite. usage: hubcommit.py <commit-message-file>"""
import glob, subprocess, sys

HUB = "/Users/petecopeland/Repos/.worktrees/trench-realsite"
S = f"{HUB}/scratch/s1002c"
msg = sys.argv[1]
for target, src, marker in ((f"{HUB}/trench/digest-realsite.md", f"{S}/digest.md", "## 2026-10-02 08:20"),
                            (f"{HUB}/trench/PLAN-realsite.md", f"{S}/plan-note.md", "**2026-10-02 08:20:")):
    cur = open(target).read()
    if marker in cur:
        print("already appended:", target)
        continue
    add = open(src).read()
    open(target, "w").write(cur.rstrip("\n") + "\n" + add)
    print("appended", len(add), "chars to", target)
paths = ["trench/digest-realsite.md", "trench/PLAN-realsite.md"]
for pat in ("scratch/s1002c/*.py", "scratch/s1002c/*.md", "scratch/s1002c/*.txt", "scratch/s1002c/*.html",
            "scratch/s1002c/*.chrome.json", "scratch/s1002c/*.chrome.png", "scratch/s1002c/bgreset-crop.png",
            "scratch/s1002c/weather-a1b1.png", "scratch/s1002c/sq-a1b1.png",
            "scratch/s1002c/github-A2-small.png", "scratch/s1002c/github-B1-small.png",
            "scratch/s0929e/camp-dev-97393a7/*.json", "scratch/s0929e/camp-dev-97393a7/ratchet.txt",
            "scratch/s0929e/camp-hex-71bfac0/*.json", "scratch/s0929e/camp-hex-71bfac0/ratchet.txt"):
    paths += [q[len(HUB) + 1:] for q in glob.glob(f"{HUB}/{pat}")]
for cmd in (["git", "add", "--"] + paths,
            ["git", "commit", "-q", "-F", msg],
            ["git", "push", "-q", "origin", "atlas/trench-realsite"],
            ["git", "log", "--oneline", "-1"],
            ["git", "status", "-sb", "--untracked-files=no"]):
    r = subprocess.run(cmd, cwd=HUB, capture_output=True, text=True)
    print(cmd[1], r.returncode, (r.stdout + r.stderr).strip()[-300:])
