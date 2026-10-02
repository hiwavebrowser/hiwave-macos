"""cargo test in a worktree against the shared warm target; prints compile errors whole, then result lines.
usage: python3 cerr.py <worktree> <cargo test args...>"""
import os, subprocess, sys, time
wt = sys.argv[1]
env = dict(os.environ, CARGO_TARGET_DIR='/Users/petecopeland/Repos/.worktrees/rs-dev-bf806c5/target')
t = time.time()
r = subprocess.run(['cargo', 'test'] + sys.argv[2:], cwd=wt, env=env, capture_output=True, text=True)
err = r.stderr
i = err.find('error[')
if i >= 0:
    print(err[i:i + 5000])
out = r.stdout
if '\nfailures:\n' in out:
    print(out[out.index('\nfailures:\n'):][:8000])
for line in (out + err).splitlines():
    if line.startswith(('test result', 'error:')):
        print(line[:300])
print(f'rc={r.returncode} {time.time()-t:.0f}s load={os.getloadavg()[0]:.1f}')
