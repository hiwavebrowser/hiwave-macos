"""Run git in the rs-dev-275d696 worktree. usage: python3 git.py <args...>"""
import subprocess, sys
r = subprocess.run(['git'] + sys.argv[1:], cwd='/Users/petecopeland/Repos/.worktrees/rs-dev-275d696',
                   capture_output=True, text=True)
print(r.stdout[-4000:], r.stderr[-2000:])
sys.exit(r.returncode)
