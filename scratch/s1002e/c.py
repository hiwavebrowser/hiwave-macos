"""cargo in a sibling worktree THROUGH cargo-serial (one build at a time across lanes, sccache),
against the lane's shared target. Prints compile errors whole, failure sections, result lines.
usage: python3 c.py <worktree name> [--touch] [--tail N] <cargo args...>
  --touch  touch every crate source first (the shared target judges freshness by mtime)"""
import os, pathlib, subprocess, sys, time
args = sys.argv[1:]
wt = '/Users/petecopeland/Repos/.worktrees/' + args.pop(0)
tail = 0
while args and args[0] in ('--touch', '--tail'):
    if args[0] == '--touch':
        now = time.time()
        for p in pathlib.Path(wt, 'crates').rglob('*.rs'):
            os.utime(p, (now, now))
        args.pop(0)
    else:
        tail = int(args[1]); args = args[2:]
env = dict(os.environ, CARGO_TARGET_DIR='/Users/petecopeland/Repos/.worktrees/rs-target')
t = time.time()
r = subprocess.run(['/Users/petecopeland/.claude/bin/cargo-serial'] + args, cwd=wt, env=env,
                   capture_output=True, text=True)
err, out = r.stderr, r.stdout
i = err.find('error')
if r.returncode and i >= 0:
    print(err[i:i + 6000])
if '\nfailures:\n' in out:
    print(out[out.index('\nfailures:\n'):][:8000])
for line in (out + err).splitlines():
    if line.startswith(('test result', 'error:', 'warning: unused')):
        print(line[:300])
if tail:
    print('\n'.join((out + err).splitlines()[-tail:]))
print(f'rc={r.returncode} {time.time()-t:.0f}s load={os.getloadavg()[0]:.1f}')
sys.exit(r.returncode)
