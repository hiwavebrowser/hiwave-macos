"""camp3.py for the `all` scope and the ratchet only (re-run after captures failed under load; the arm's
micro and builtins results stay). Retries `all` once more if any case is still unmeasured.
usage: camp_all.py <label> <binary>"""
import json, os, shutil, subprocess, sys, time
SRC = '/Users/petecopeland/Repos/.worktrees/rs-dev-9f49a40'
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
label, binary = sys.argv[1], sys.argv[2]
shutil.copy2(binary, f'{SRC}/target/release/parity-capture')
stub = f'{HUB}/scratch/s0929c/stubbin'
env = dict(os.environ, PATH=stub + ':' + os.environ['PATH'])
env.pop('CARGO_TARGET_DIR', None)
out = f'{HUB}/scratch/s0929e/camp-{label}'
t = time.time()
for attempt in (1, 2):
    r = subprocess.run(['python3', 'scripts/parity_test.py', '--scope', 'all'], cwd=SRC, env=env,
                       capture_output=True, text=True)
    open(f'{out}/all.log', 'w').write(r.stdout + r.stderr)
    shutil.copy2(f'{SRC}/parity-baseline/parity_test_results.json', f'{out}/all.json')
    bad = [x['case_id'] for x in json.load(open(f'{out}/all.json'))['results']
           if not isinstance(x.get('diff_pct'), (int, float))]
    print('all attempt', attempt, 'rc', r.returncode, 'unmeasured', bad, flush=True)
    if not bad:
        break
r = subprocess.run(['python3', f'{HUB}/scratch/shelf302/ratchet_local.py', SRC, label], cwd=SRC, env=env,
                   capture_output=True, text=True)
open(f'{out}/ratchet.txt', 'w').write(r.stdout + r.stderr)
print('ratchet rc', r.returncode, '\n'.join((r.stdout + r.stderr).splitlines()[-4:]))
print(f'{time.time()-t:.0f}s load={os.getloadavg()[0]:.1f}')
print('CAMPALLDONE', flush=True)
