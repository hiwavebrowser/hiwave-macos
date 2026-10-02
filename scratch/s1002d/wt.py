"""Run one command inside a sibling worktree (the shell refuses `cd <worktree> && git/cargo ...`).
usage: python3 wt.py <worktree name under ~/Repos/.worktrees> [--tail N] <cmd> [args...]
Prints the command's output (the last N lines with --tail) and exits with its status.
CARGO_TARGET_DIR is the shared warm target unless already set."""
import os, subprocess, sys, time
args = sys.argv[1:]
wt = '/Users/petecopeland/Repos/.worktrees/' + args.pop(0)
tail = None
if args[0] == '--tail':
    tail = int(args[1]); args = args[2:]
env = dict(os.environ)
env.setdefault('CARGO_TARGET_DIR', '/Users/petecopeland/Repos/.worktrees/rs-dev-bf806c5/target')
t = time.time()
r = subprocess.run(args, cwd=wt, env=env, capture_output=True, text=True)
out = (r.stdout + r.stderr).splitlines()
print('\n'.join(out[-tail:] if tail else out))
if time.time() - t > 5:
    print(f'[{time.time()-t:.0f}s rc={r.returncode} load={os.getloadavg()[0]:.1f}]')
sys.exit(r.returncode)
