#!/usr/bin/env python3
"""Run a worktree's scripts/parity_test.py unchanged, except its one
`cargo build --release -p parity-capture` step is skipped: the caller has
already placed that commit's release binary at <worktree>/target/release/.

    cs-receipt-nobuild.py <worktree> [parity_test args...]
"""
import os
import subprocess
import sys

worktree = os.path.abspath(sys.argv[1])
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
