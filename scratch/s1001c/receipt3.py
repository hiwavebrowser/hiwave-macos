"""Campaign receipt table for two camp arms (scratch/s0929e/camp-<arm>), scopes all, builtins, micro.
usage: receipt3.py <dev-arm> <fix-arm>"""
import json, sys
D = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s0929e'
dev, fix = sys.argv[1:3]


def load(arm, scope):
    j = json.load(open(f'{D}/camp-{arm}/{scope}.json'))
    rs = j.get('results') or j.get('cases') or j
    rs = rs if isinstance(rs, list) else list(rs.values())
    vals = {}
    for r in rs:
        v = r.get('diff_pct')
        vals[r.get('case_id') or r.get('case') or r.get('name')] = float('nan') if v is None else v
    return j.get('timestamp', '')[:19], vals, sum(1 for r in rs if r.get('passed'))


def avg(v):
    xs = [x for x in v.values() if x == x]
    return sum(xs) / len(xs) if xs else float('nan')


for scope in ('all', 'builtins', 'micro'):
    ta, a, pa = load(dev, scope)
    tb, b, pb = load(fix, scope)
    same = sum(1 for k in a if abs(a[k] - b.get(k, -1)) < 1e-9)
    print(f'{scope}: develop {ta} {pa}/{len(a)} avg {avg(a):.4f} | fix {tb} {pb}/{len(b)} '
          f'avg {avg(b):.4f} | identical {same}/{len(a)}')
    for k in sorted(a):
        mark = '' if abs(a[k] - b.get(k, -1)) < 1e-9 else '  <-- moved'
        print(f'| {k} | {a[k]:.4f} | {b.get(k, float("nan")):.4f} |{mark}')
ra = open(f'{D}/camp-{dev}/ratchet.txt').read().splitlines()
rb = open(f'{D}/camp-{fix}/ratchet.txt').read().splitlines()
print('ratchet identical:', ra == rb)
for x, y in zip(ra, rb):
    if x != y:
        print('  dev:', x.strip())
        print('  fix:', y.strip())
