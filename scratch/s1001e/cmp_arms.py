"""Compare two campaign arms' diff_pct per case. usage: cmp_arms.py <dev-arm> <fix-arm>"""
import json, sys
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s0929e'
dev, fix = sys.argv[1:3]
for scope in ('micro', 'builtins', 'all'):
    try:
        a = {r['case_id']: r['diff_pct'] for r in json.load(open(f'{HUB}/camp-{dev}/{scope}.json'))['results']}
        b = {r['case_id']: r['diff_pct'] for r in json.load(open(f'{HUB}/camp-{fix}/{scope}.json'))['results']}
    except FileNotFoundError as e:
        print(scope, 'missing', e)
        continue
    moved = [(k, a[k], b.get(k)) for k in sorted(a) if a[k] != b.get(k)]
    print(f'{scope}: {len(a) - len(moved)}/{len(a)} identical; avg {sum(a.values())/len(a):.4f} -> '
          f'{sum(b.values())/len(b):.4f}')
    for k, x, y in moved:
        print(f'   {k}: {x} -> {y}')
