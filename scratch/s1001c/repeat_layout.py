"""Run a worktree's rustkit-layout lib suite N times in parallel mode and print each result line.
usage: repeat_layout.py <worktree> <n>"""
import os, pathlib, subprocess, sys, time
wt, n = sys.argv[1], int(sys.argv[2])
env = dict(os.environ, CARGO_TARGET_DIR='/Users/petecopeland/Repos/.worktrees/rs-dev-bf806c5/target')
now = time.time()
for p in pathlib.Path(wt, 'crates').rglob('*.rs'):
    os.utime(p, (now, now))
for i in range(n):
    r = subprocess.run(['cargo', 'test', '-p', 'rustkit-layout', '--lib'], cwd=wt, env=env, capture_output=True, text=True)
    lines = [l for l in (r.stdout + r.stderr).splitlines() if l.startswith(('test result', 'error')) or 'FAILED' in l]
    print(i + 1, 'rc', r.returncode, ' | '.join(lines)[:400], f'load={os.getloadavg()[0]:.1f}', flush=True)
