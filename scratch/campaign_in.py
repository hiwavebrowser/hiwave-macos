"""Run scripts/parity_test.py in <worktree> with CARGO_TARGET_DIR=<target_dir>; save output to <out>.
usage: campaign_in.py <worktree> <target_dir> <out>"""
import os, subprocess, sys

env = dict(os.environ, CARGO_TARGET_DIR=sys.argv[2])
p = subprocess.run(["python3", "scripts/parity_test.py"], cwd=sys.argv[1], env=env, capture_output=True, text=True)
open(sys.argv[3], "w").write(p.stdout + p.stderr)
print("\n".join((p.stdout + p.stderr).strip().splitlines()[-12:]))
