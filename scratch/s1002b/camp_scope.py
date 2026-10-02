"""Re-run one campaign scope for an arm whose capture failed under load (camp3.py's staging, one scope,
no ratchet). Never run it while another arm's campaign is running: both stage a binary in the same place.
usage: camp_scope.py <label> <binary> <scope>"""
import os, shutil, subprocess, sys
SRC = '/Users/petecopeland/Repos/.worktrees/rs-dev-9f49a40'
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
label, binary, scope = sys.argv[1:4]
shutil.copy2(binary, f'{SRC}/target/release/parity-capture')
env = dict(os.environ, PATH=f'{HUB}/scratch/s0929c/stubbin:' + os.environ['PATH'])
env.pop('CARGO_TARGET_DIR', None)
out = f'{HUB}/scratch/s0929e/camp-{label}'
r = subprocess.run(['python3', 'scripts/parity_test.py', '--scope', scope], cwd=SRC, env=env,
                   capture_output=True, text=True)
open(f'{out}/{scope}.log', 'w').write(r.stdout + r.stderr)
shutil.copy2(f'{SRC}/parity-baseline/parity_test_results.json', f'{out}/{scope}.json')
print(scope, 'rc', r.returncode, '\n'.join(r.stdout.splitlines()[-12:]))
