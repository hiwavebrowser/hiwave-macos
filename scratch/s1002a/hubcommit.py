"""Append this session's digest section and PLAN note (once), commit the session's hub artifacts and push
atlas/trench-realsite. usage: hubcommit.py <commit-message-file>"""
import glob, subprocess, sys

HUB = "/Users/petecopeland/Repos/.worktrees/trench-realsite"
S = f"{HUB}/scratch/s1002a"
msg = sys.argv[1]
for target, src, marker in ((f"{HUB}/trench/digest-realsite.md", f"{S}/digest.md", "## 2026-10-02 02:10"),
                            (f"{HUB}/trench/PLAN-realsite.md", f"{S}/plan-note.md", "**2026-10-02 02:10:")):
    cur = open(target).read()
    if marker in cur:
        print("already appended:", target)
        continue
    add = open(src).read()
    open(target, "w").write(cur.rstrip("\n") + "\n" + add)
    print("appended", len(add), "chars to", target)
paths = ["trench/digest-realsite.md", "trench/PLAN-realsite.md"]
for pat in ("scratch/s1002a/*.py", "scratch/s1002a/*.md", "scratch/s1002a/*.txt", "scratch/s1002a/*.html",
            "scratch/s1002a/*.chrome.json",
            "scratch/s0929e/camp-dev-f60d1d6/*.json", "scratch/s0929e/camp-dev-f60d1d6/ratchet.txt",
            "scratch/s0929e/camp-btn-19ec405/*.json", "scratch/s0929e/camp-btn-19ec405/ratchet.txt"):
    paths += [q[len(HUB) + 1:] for q in glob.glob(f"{HUB}/{pat}")]
for cmd in (["git", "add", "--"] + paths,
            ["git", "commit", "-q", "-F", msg],
            ["git", "push", "-q", "origin", "atlas/trench-realsite"],
            ["git", "log", "--oneline", "-1"],
            ["git", "status", "-sb", "--untracked-files=no"]):
    r = subprocess.run(cmd, cwd=HUB, capture_output=True, text=True)
    print(cmd[1], r.returncode, (r.stdout + r.stderr).strip()[-300:])
