#!/usr/bin/env python3
"""Cascade lane cargo runner: run cargo in <worktree> with
CARGO_TARGET_DIR=cascade-target (the lane allowlist has no bare cargo).

    cs_cargo.py <worktree> <cargo args...>
"""
import os
import subprocess
import sys

os.environ["CARGO_TARGET_DIR"] = "/Users/petecopeland/Repos/.worktrees/cascade-target"
os.chdir(os.path.abspath(sys.argv[1]))
sys.exit(subprocess.run(["cargo"] + sys.argv[2:]).returncode)
