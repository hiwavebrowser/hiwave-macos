"""Run one chunk of the top-20 board into a run dir (chunks fit the 10-min tool cap).
usage: python3 chunk.py <run-dir-name> <binary> <chunk 1-4 | site,site,...> [--summarize]"""
import os, subprocess, sys, time
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
CHUNKS = {'1': 'google youtube facebook instagram wikipedia',
          '2': 'lyft reddit x linkedin yahoo',
          '3': 'bing walmart microsoft apple netflix',
          '4': 'github shopify squarespace cnn weather'}
run, binp, which = sys.argv[1:4]
sites = CHUNKS.get(which, which.replace(',', ' ')).split()
out = f'{HUB}/trench/realsite/runs/{run}'
args = ['python3', 'scripts/realsite_board.py', '--capture-bin', os.path.abspath(binp), '--out', out]
if '--summarize' in sys.argv:
    args.append('--summarize')
else:
    for s in sites:
        args += ['--site', s]
t = time.time()
print(f'load={os.getloadavg()[0]:.1f}', flush=True)
p = subprocess.run(args, cwd=HUB, capture_output=True, text=True)
print('\n'.join((p.stdout + p.stderr).splitlines()[-30:]))
print(f'rc={p.returncode} {time.time()-t:.0f}s load={os.getloadavg()[0]:.1f}')
