"""Append this session's digest and PLAN note, commit the hub artifacts, push atlas/trench-realsite.
usage: hubcommit.py <HH:MM> <commit-message-file> <ci-status-file>"""
import glob, subprocess, sys

HUB = "/Users/petecopeland/Repos/.worktrees/trench-realsite"
S = f"{HUB}/scratch/s1001f"
stamp, msg, ci = sys.argv[1], sys.argv[2], open(sys.argv[3]).read().strip()

digest = open(f"{S}/digest.md").read().replace("__STAMP__", stamp).replace("__CI__", ci)
assert "__" not in digest, [w for w in digest.split() if "__" in w]
d = f"{HUB}/trench/digest-realsite.md"
cur = open(d).read()
if digest.strip() not in cur:
    open(d, "w").write(cur.rstrip("\n") + "\n" + digest)

note = open(f"{S}/plan-note.md").read().replace("__STAMP__", f"2026-10-01 {stamp}")
assert "__" not in note, [w for w in note.split() if "__" in w]
p = f"{HUB}/trench/PLAN-realsite.md"
cur = open(p).read()
if note.strip() not in cur:
    open(p, "w").write(cur.rstrip("\n") + "\n" + note)

paths = ["trench/digest-realsite.md", "trench/PLAN-realsite.md"]
for pat in ("scratch/s1001f/*.py", "scratch/s1001f/*.md", "scratch/s1001f/*.txt",
            "scratch/s0929e/camp-dev-a0583fa/*.json", "scratch/s0929e/camp-dev-a0583fa/ratchet.txt",
            "scratch/s0929e/camp-generic-5ad75d9/*.json", "scratch/s0929e/camp-generic-5ad75d9/ratchet.txt"):
    paths += [q[len(HUB) + 1:] for q in glob.glob(f"{HUB}/{pat}")]
for cmd in (["git", "add", "--"] + paths,
            ["git", "commit", "-q", "-F", msg],
            ["git", "push", "-q", "origin", "atlas/trench-realsite"],
            ["git", "log", "--oneline", "-1"],
            ["git", "status", "-sb", "--untracked-files=no"]):
    r = subprocess.run(cmd, cwd=HUB, capture_output=True, text=True)
    print(cmd[1], r.returncode, (r.stdout + r.stderr).strip()[-300:])
