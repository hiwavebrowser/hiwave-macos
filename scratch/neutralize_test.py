"""Temporarily swap a line in a file, run cargo test, restore, print test lines.
usage: neutralize_test.py <worktree> <file> <from> <to> <cargo test args...>"""
import os, subprocess, sys

wt, rel, frm, to, *args = sys.argv[1:]
path = os.path.join(wt, rel)
orig = open(path).read()
assert orig.count(frm) == 1, "anchor not unique"
open(path, "w").write(orig.replace(frm, to))
try:
    env = dict(os.environ, CARGO_TARGET_DIR="/Users/petecopeland/Repos/.worktrees/rs-dev-bf806c5/target")
    p = subprocess.run(["cargo", "test", *args], cwd=wt, env=env, capture_output=True, text=True)
finally:
    open(path, "w").write(orig)
for l in (p.stdout + p.stderr).splitlines():
    if l.startswith("test ") or "test result" in l or l.startswith("error"):
        print(l)
print("restored", open(path).read() == orig)
