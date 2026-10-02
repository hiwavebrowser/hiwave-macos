"""Fail-first for the four-digit hex fix: put the fix branch's engine lib.rs (it differs from develop only
by the new test) into the develop worktree, run the test against develop's rustkit-css, restore.
usage: failfirst_hex.py <develop-worktree> <fix-worktree>"""
import os, shutil, subprocess, sys, time
dev, fix = sys.argv[1:3]
rel = 'crates/rustkit-engine/src/lib.rs'
d = subprocess.run(['git', 'diff', '--stat', 'HEAD'], cwd=dev, capture_output=True, text=True).stdout
assert not d.strip(), 'develop worktree is dirty: ' + d
shutil.copy2(f'{fix}/{rel}', f'{dev}/{rel}')
now = time.time()
os.utime(f'{dev}/{rel}', (now, now))
os.utime(f'{dev}/crates/rustkit-css/src/lib.rs', (now, now))
env = dict(os.environ, CARGO_TARGET_DIR='/Users/petecopeland/Repos/.worktrees/rs-dev-bf806c5/target')
try:
    r = subprocess.run(['cargo', 'test', '-p', 'rustkit-engine', '--lib', 'four_digit'], cwd=dev, env=env,
                       capture_output=True, text=True)
    out = r.stdout
    if '\nfailures:\n' in out:
        print(out[out.index('\nfailures:\n'):][:2500])
    for line in (out + r.stderr).splitlines():
        if line.startswith(('test result', 'error')):
            print(line[:300])
    print('rc', r.returncode)
finally:
    subprocess.run(['git', 'checkout', '--', rel], cwd=dev, check=True)
    print('restored:', subprocess.run(['git', 'status', '--short'], cwd=dev, capture_output=True, text=True).stdout or 'clean')
