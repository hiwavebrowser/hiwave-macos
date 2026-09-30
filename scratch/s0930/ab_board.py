"""Interleaved A/B on the real-site board for a few sites.
usage: python3 ab_board.py <tag> <sites,comma> <binA> <binB> [rounds=2]
Runs A,B,A,B... into trench/realsite/runs/<tag>-<n>-<A|B>/ and prints each site row."""
import json, os, subprocess, sys, time
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
tag, sites, a, b = sys.argv[1:5]
rounds = int(sys.argv[5]) if len(sys.argv) > 5 else 2
n = 0
for r in range(rounds):
    for arm, binp in (('A', a), ('B', b)):
        n += 1
        out = f'{HUB}/trench/realsite/runs/{tag}-{n}-{arm}'
        args = ['python3', 'scripts/realsite_board.py', '--capture-bin', os.path.abspath(binp), '--out', out]
        for s in sites.split(','):
            args += ['--site', s]
        t = time.time()
        p = subprocess.run(args, cwd=HUB, capture_output=True, text=True)
        for s in sites.split(','):
            try:
                d = json.load(open(f'{out}/{s}.json'))
                ch = d.get('checks', d)
                print(arm, n, s, json.dumps({k: ch.get(k) for k in ('points', 'loads', 'readable', 'looks_right', 'readable_pct', 'diff_pct', 'looks_right_pct')})[:300])
            except Exception as e:
                print(arm, n, s, 'no json', e, p.stdout[-400:], p.stderr[-400:])
        print(f'  {time.time()-t:.0f}s load={os.getloadavg()[0]:.1f}', flush=True)
