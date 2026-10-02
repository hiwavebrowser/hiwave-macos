"""Append this session's digest section and PLAN note (once), commit the session's hub artifacts and push
atlas/trench-realsite. usage: hubcommit.py <commit-message-file>"""
import glob, subprocess, sys

HUB = "/Users/petecopeland/Repos/.worktrees/trench-realsite"
S = f"{HUB}/scratch/s1002d"
msg = sys.argv[1]
note = open(f"{S}/plan-note.md").read().replace(
    "`atlas/rs-image-loader-routing` @ ef58355 (PRSTATE)", "#438 (`atlas/rs-image-loader-routing` @ ef58355, open)")
note = note.replace(
    "If the routing branch has no PR yet, its receipts are the first thing: `python3 scratch/s1002d/img_receipts.py`",
    "#438's receipts were made with `python3 scratch/s1002d/img_receipts.py`")
assert "PRSTATE" not in note
for target, add, marker in ((f"{HUB}/trench/digest-realsite.md", open(f"{S}/digest.md").read(), "## 2026-10-02 11:20"),
                            (f"{HUB}/trench/PLAN-realsite.md", note, "**2026-10-02 11:20:")):
    cur = open(target).read()
    if marker in cur:
        print("already appended:", target)
        continue
    open(target, "w").write(cur.rstrip("\n") + "\n" + add)
    print("appended", len(add), "chars to", target)
paths = ["trench/digest-realsite.md", "trench/PLAN-realsite.md"]
for pat in ("scratch/s1002d/*.py", "scratch/s1002d/*.md", "scratch/s1002d/*.txt", "scratch/s1002d/*.png"):
    paths += [p[len(HUB) + 1:] for p in glob.glob(f"{HUB}/{pat}")]
subprocess.run(["git", "add", "-f"] + paths, cwd=HUB, check=True)
subprocess.run(["git", "commit", "-q", "-F", msg], cwd=HUB, check=True)
print(subprocess.run(["git", "log", "--oneline", "-1"], cwd=HUB, capture_output=True, text=True).stdout)
r = subprocess.run(["git", "push", "origin", "atlas/trench-realsite"], cwd=HUB, capture_output=True, text=True)
print((r.stdout + r.stderr)[-400:])
