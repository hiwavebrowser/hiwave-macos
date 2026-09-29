"""Run cargo in the rs-dev-275d696 worktree with the warm target dir.
usage: python3 cargo.py <cargo args...>   (prints the last 40 lines)"""
import os, subprocess, sys, time
SRC = '/Users/petecopeland/Repos/.worktrees/rs-dev-275d696'
env = dict(os.environ, CARGO_TARGET_DIR='/Users/petecopeland/Repos/.worktrees/rs-dev-bf806c5/target')
t = time.time()
r = subprocess.run(['cargo'] + sys.argv[1:], cwd=SRC, env=env, capture_output=True, text=True)
lines = (r.stdout + '\n' + r.stderr).splitlines()
keep = [l for l in lines if l.startswith(('test ', 'test result', 'thread ', 'error', 'failures')) or 'assert' in l or 'left' in l or 'right' in l]
print('\n'.join(keep[-60:]))
print(f'rc={r.returncode} {time.time()-t:.0f}s load={os.getloadavg()[0]:.1f}')
