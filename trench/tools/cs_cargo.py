#!/usr/bin/env python3
"""Cascade lane cargo runner: run cargo in <worktree> with
CARGO_TARGET_DIR=cascade-target (the lane allowlist has no bare cargo).

    cs_cargo.py <worktree> <cargo args...>

Runs through ~/.claude/bin/cargo-serial (2026-10-02): one build at a time
across every lane on this Mac, with sccache. Expect a wait for the lock.
"""
import os
import subprocess
import sys

os.environ["CARGO_TARGET_DIR"] = "/Users/petecopeland/Repos/.worktrees/cascade-target"
os.chdir(os.path.abspath(sys.argv[1]))
sys.exit(subprocess.run(["/Users/petecopeland/.claude/bin/cargo-serial"] + sys.argv[2:]).returncode)
