#!/usr/bin/env python3
"""Run git in another worktree without `cd`/`git -C` (both need approval in the headless lane).

usage: wt_git.py <worktree> <git args...>
"""
import subprocess
import sys

if len(sys.argv) < 3:
    sys.exit(__doc__)
sys.exit(subprocess.run(["git", *sys.argv[2:]], cwd=sys.argv[1]).returncode)
