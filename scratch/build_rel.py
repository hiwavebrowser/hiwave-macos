"""Release-build parity-capture in <worktree>, reusing another tree's target dir for speed.
usage: build_rel.py <worktree> <target_dir>"""
import os, subprocess, sys

env = dict(os.environ, CARGO_TARGET_DIR=sys.argv[2])
p = subprocess.run(["cargo", "build", "--release", "-p", "parity-capture"], cwd=sys.argv[1], env=env,
                   capture_output=True, text=True)
print("\n".join(p.stderr.strip().splitlines()[-3:]))
sys.exit(p.returncode)
