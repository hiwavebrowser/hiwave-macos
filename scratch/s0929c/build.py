"""Release parity-capture for a source worktree, reusing a warm target dir for deps.

usage: python3 build.py <src-worktree> <out-name> [target-dir]
"""
import os, shutil, subprocess, sys, time

src, name = sys.argv[1], sys.argv[2]
tgt = sys.argv[3] if len(sys.argv) > 3 else "/Users/petecopeland/Repos/.worktrees/rs-dev-bf806c5/target"
env = dict(os.environ, CARGO_TARGET_DIR=tgt)
subprocess.run(["git", "log", "--oneline", "-1"], cwd=src)
t = time.time()
r = subprocess.run(["cargo", "build", "--release", "-p", "parity-capture"], cwd=src, env=env,
                   capture_output=True, text=True)
print("\n".join((r.stdout + r.stderr).splitlines()[-5:]))
print(f"build rc={r.returncode} {time.time()-t:.0f}s")
if r.returncode:
    sys.exit(r.returncode)
out = f"/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/bin/{name}"
shutil.copy2(f"{tgt}/release/parity-capture", out)
print("->", out)
print(os.getloadavg())
