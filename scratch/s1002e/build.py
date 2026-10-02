"""Build parity-capture in a worktree through cargo-serial against the lane's shared target,
touching every crate source first, and copy the binary out.
usage: python3 build.py <worktree name> <out-binary>
Uses --profile parity when the worktree's Cargo.toml has [profile.parity], else --release."""
import os, pathlib, shutil, subprocess, sys, time
wt = '/Users/petecopeland/Repos/.worktrees/' + sys.argv[1]
out = sys.argv[2]
target = '/Users/petecopeland/Repos/.worktrees/rs-target'
parity = '[profile.parity]' in pathlib.Path(wt, 'Cargo.toml').read_text()
now = time.time()
n = 0
for p in pathlib.Path(wt, 'crates').rglob('*.rs'):
    os.utime(p, (now, now)); n += 1
t = time.time()
prof = ['--profile', 'parity'] if parity else ['--release']
r = subprocess.run(['/Users/petecopeland/.claude/bin/cargo-serial', 'build'] + prof + ['-p', 'parity-capture'],
                   cwd=wt, env=dict(os.environ, CARGO_TARGET_DIR=target), capture_output=True, text=True)
print('\n'.join((r.stdout + r.stderr).splitlines()[-4:]))
print(f'touched {n} files; rc={r.returncode} {time.time()-t:.0f}s load={os.getloadavg()[0]:.1f}')
if r.returncode == 0:
    shutil.copy2(f'{target}/{"parity" if parity else "release"}/parity-capture', out)
    print('->', out)
sys.exit(r.returncode)
