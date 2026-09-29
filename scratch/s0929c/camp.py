"""Campaign A/B in rs-dev-275d696: stage each binary as target/release/parity-capture,
run parity_test.py (all + builtins scope), keep results per arm, then the ratchet.
usage: python3 camp.py <label> <binary>"""
import json, os, shutil, subprocess, sys, time
SRC = '/Users/petecopeland/Repos/.worktrees/rs-dev-275d696'
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
label, binary = sys.argv[1], sys.argv[2]
os.makedirs(f'{SRC}/target/release', exist_ok=True)
shutil.copy2(binary, f'{SRC}/target/release/parity-capture')
env = dict(os.environ, CARGO_TARGET_DIR=f'{SRC}/target')  # cargo build no-ops? only if fresh; see below
# parity_test.py calls `cargo build`; point it at a stub cargo that succeeds so the staged binary is used.
stub = f'{HUB}/scratch/s0929c/stubbin'
os.makedirs(stub, exist_ok=True)
with open(f'{stub}/cargo', 'w') as f:
    f.write('#!/bin/sh\nexit 0\n')
os.chmod(f'{stub}/cargo', 0o755)
env = dict(os.environ, PATH=stub + ':' + os.environ['PATH'])
t = time.time()
out = f'{HUB}/scratch/s0929c/camp-{label}'
os.makedirs(out, exist_ok=True)
for scope in ('all', 'builtins'):
    r = subprocess.run(['python3', 'scripts/parity_test.py', '--scope', scope], cwd=SRC, env=env,
                       capture_output=True, text=True)
    open(f'{out}/{scope}.log', 'w').write(r.stdout + r.stderr)
    shutil.copy2(f'{SRC}/parity-baseline/parity_test_results.json', f'{out}/{scope}.json')
    print(scope, 'rc', r.returncode, '\n'.join(r.stdout.splitlines()[-6:]))
r = subprocess.run(['python3', f'{HUB}/scratch/shelf302/ratchet_local.py', SRC, label], cwd=SRC, env=env,
                   capture_output=True, text=True)
open(f'{out}/ratchet.txt', 'w').write(r.stdout + r.stderr)
print('ratchet rc', r.returncode, '\n'.join((r.stdout + r.stderr).splitlines()[-8:]))
print(f'{time.time()-t:.0f}s')
