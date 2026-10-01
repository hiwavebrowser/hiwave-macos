"""Run git in <worktree>; print the tail of its output.
usage: git_in.py <worktree> <git args...>"""
import subprocess, sys
p = subprocess.run(["git", *sys.argv[2:]], cwd=sys.argv[1], capture_output=True, text=True)
print((p.stdout + p.stderr).strip()[-6000:])
print("rc", p.returncode)
sys.exit(p.returncode)
