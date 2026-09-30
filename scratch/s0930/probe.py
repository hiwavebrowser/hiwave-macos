"""usage: probe.py <filter> [worktree] — rustkit-engine lib tests, warm debug target, filtered output."""
import os, re, subprocess, sys, time
flt = sys.argv[1]
wt = sys.argv[2] if len(sys.argv) > 2 else '/Users/petecopeland/Repos/.worktrees/rs-flex-collapse-through'
pkg = os.environ.get('PKG', 'rustkit-engine')
env = dict(os.environ, CARGO_TARGET_DIR='/Users/petecopeland/Repos/.worktrees/rs-dev-275d696/target')
t = time.time()
r = subprocess.run(['cargo', 'test', '-p', pkg, '--lib', flt, '--', '--nocapture', '--test-threads=1'],
                   cwd=wt, env=env, capture_output=True, text=True)
pat = re.compile(r'\[[01]\]|test result|^error|panicked|FAILED|^test |left|right')
lines = [l for l in (r.stdout + r.stderr).splitlines() if pat.search(l)]
print('\n'.join(lines[:120]))
print(f'rc={r.returncode} {time.time()-t:.0f}s load={os.getloadavg()[0]:.1f}')
