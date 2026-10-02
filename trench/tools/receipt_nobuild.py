#!/usr/bin/env python3
"""Run a worktree's scripts/parity_test.py unchanged, except its one
`cargo build --release -p parity-capture` step is skipped: the caller has
already placed that commit's release binary at <worktree>/target/release/.

    cs-receipt-nobuild.py <worktree> [NAME=VALUE ...] [parity_test args...]

Leading NAME=VALUE arguments set engine flags for the run (the headless lane
cannot prefix a command with an env assignment).
"""
import os
import subprocess
import sys

worktree = os.path.abspath(sys.argv[1])
while len(sys.argv) > 2 and "=" in sys.argv[2] and not sys.argv[2].startswith("-"):
    name, _, value = sys.argv.pop(2).partition("=")
    os.environ[name] = value
    print(f"== env: {name}={value}", flush=True)
assert os.path.exists(os.path.join(worktree, "target/release/parity-capture")), "binary missing"
sys.path.insert(0, os.path.join(worktree, "scripts"))
os.chdir(worktree)

_real_run = subprocess.run


def _run(cmd, *a, **kw):
    if list(cmd[:4]) == ["cargo", "build", "--release", "-p"]:
        return subprocess.CompletedProcess(cmd, 0, "", "")
    return _real_run(cmd, *a, **kw)


subprocess.run = _run
import parity_test  # noqa: E402

sys.argv = ["parity_test.py"] + sys.argv[2:]
parity_test.main()
