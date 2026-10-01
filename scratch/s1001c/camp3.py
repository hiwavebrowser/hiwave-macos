"""Campaign arm with the micro scope added: stage a binary as <SRC>/target/release/parity-capture, run
parity_test.py (all + builtins + micro), keep results per arm under scratch/s0929e/camp-<label>, then
the ratchet (which reads the last `all` results, so `all` runs last).
usage: python3 camp3.py <label> <binary>   (SRC = the develop worktree, same fixtures for both arms)"""
import os, shutil, subprocess, sys, time
SRC = '/Users/petecopeland/Repos/.worktrees/rs-dev-9f49a40'
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
label, binary = sys.argv[1], sys.argv[2]
os.makedirs(f'{SRC}/target/release', exist_ok=True)
shutil.copy2(binary, f'{SRC}/target/release/parity-capture')
stub = f'{HUB}/scratch/s0929c/stubbin'  # a `cargo` that exits 0, so the staged binary is used
env = dict(os.environ, PATH=stub + ':' + os.environ['PATH'])
env.pop('CARGO_TARGET_DIR', None)
t = time.time()
out = f'{HUB}/scratch/s0929e/camp-{label}'
os.makedirs(out, exist_ok=True)
for scope in ('micro', 'builtins', 'all'):
    r = subprocess.run(['python3', 'scripts/parity_test.py', '--scope', scope], cwd=SRC, env=env,
                       capture_output=True, text=True)
    open(f'{out}/{scope}.log', 'w').write(r.stdout + r.stderr)
    shutil.copy2(f'{SRC}/parity-baseline/parity_test_results.json', f'{out}/{scope}.json')
    print(scope, 'rc', r.returncode, '\n'.join(r.stdout.splitlines()[-4:]))
    if scope == 'micro':
        cap = f'{SRC}/parity-baseline/captures/rounded-corners'
        if os.path.isdir(cap):
            shutil.copytree(cap, f'{out}/rounded-corners', dirs_exist_ok=True)
r = subprocess.run(['python3', f'{HUB}/scratch/shelf302/ratchet_local.py', SRC, label], cwd=SRC, env=env,
                   capture_output=True, text=True)
open(f'{out}/ratchet.txt', 'w').write(r.stdout + r.stderr)
print('ratchet rc', r.returncode, '\n'.join((r.stdout + r.stderr).splitlines()[-8:]))
print(f'{time.time()-t:.0f}s load={os.getloadavg()[0]:.1f}')
