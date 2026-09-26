"""Build an unstripped, symbolized parity-capture for `sample`. usage: build_prof.py <worktree>
Output: <worktree>/target-prof/release/parity-capture"""
import os, subprocess, sys

env = dict(os.environ, CARGO_PROFILE_RELEASE_STRIP="false", CARGO_PROFILE_RELEASE_DEBUG="line-tables-only",
           CARGO_PROFILE_RELEASE_LTO="false", CARGO_PROFILE_RELEASE_CODEGEN_UNITS="16", CARGO_TARGET_DIR="target-prof")
p = subprocess.run(["cargo", "build", "--release", "-p", "parity-capture"], cwd=sys.argv[1], env=env,
                   capture_output=True, text=True)
print("\n".join(p.stderr.strip().splitlines()[-3:]))
sys.exit(p.returncode)
