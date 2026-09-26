"""Run cargo in <worktree> with the shared target dir; print filtered output.
usage: cargo_in.py <worktree> <cargo args...>"""
import os, re, subprocess, sys

TARGET = "/Users/petecopeland/Repos/.worktrees/rs-dev-bf806c5/target"
env = dict(os.environ, CARGO_TARGET_DIR=TARGET)
p = subprocess.run(["cargo", *sys.argv[2:]], cwd=sys.argv[1], env=env, capture_output=True, text=True)
keep = re.compile(r"^(error|test |test result|failures:|---- |thread .* panicked|\s+left|\s+right|.*assert)")
lines = (p.stdout + "\n" + p.stderr).splitlines()
out = []
for i, l in enumerate(lines):
    if keep.match(l):
        out.append(l)
        if l.startswith("error"):
            out.extend(lines[i + 1:i + 4])
print("\n".join(out[-120:]))
print("rc", p.returncode)
sys.exit(p.returncode)
