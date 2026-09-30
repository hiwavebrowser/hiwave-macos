"""Run a shell command string in a directory, full output.
usage: python3 sh.py <dir> '<shell command>'
CARGO_TARGET_DIR defaults to <dir>/target unless set."""
import os, subprocess, sys, time
d = sys.argv[1]
env = dict(os.environ)
env.setdefault('CARGO_TARGET_DIR', os.path.join(d, 'target'))
t = time.time()
r = subprocess.run(sys.argv[2], shell=True, cwd=d, env=env, capture_output=True, text=True)
out = (r.stdout + '\n' + r.stderr).splitlines()
n = int(os.environ.get('TAIL', '400'))
print('\n'.join(out[-n:]))
print(f'rc={r.returncode} {time.time()-t:.0f}s load={os.getloadavg()[0]:.1f}')
sys.exit(r.returncode)
