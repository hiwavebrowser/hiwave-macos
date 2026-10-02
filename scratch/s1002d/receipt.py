"""Campaign receipt (markdown) for two camp3.py arms: summary line per scope, the ratchet lines that
differ, and the per-case tables.
usage: receipt.py <develop label> <fix label> <develop sha> <fix sha>"""
import json, sys
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s0929e'
dl, fl, dsha, fsha = sys.argv[1:5]


def load(label, scope):
    j = json.load(open(f'{HUB}/camp-{label}/{scope}.json'))
    cases = {}
    for r in j['results']:
        p = r.get('pixel') or {}
        cases[r['case_id']] = p.get('diffPercent')
    return j, cases


def fmt(v):
    return 'NOT-MEASURED' if v is None else f'{v:.4f}'


tables = {}
print('```')
for scope in ('all', 'builtins', 'micro'):
    dj, dc = load(dl, scope)
    fj, fc = load(fl, scope)
    same = sum(1 for k in dc if fc.get(k) is not None and dc[k] is not None and abs(dc[k] - fc[k]) < 5e-5)
    avg = lambda c: sum(v for v in c.values() if v is not None) / max(1, sum(v is not None for v in c.values()))
    n = len(dc)
    print(f"{scope + ':':10}develop {dj['timestamp'][:19]} {dj['passed']}/{n} avg {avg(dc):.4f} | "
          f"fix {fj['timestamp'][:19]} {fj['passed']}/{len(fc)} avg {avg(fc):.4f} | {same}/{n} identical")
    tables[scope] = (dc, fc)
print('```')
dr = [l for l in open(f'{HUB}/camp-{dl}/ratchet.txt').read().splitlines()]
fr = [l for l in open(f'{HUB}/camp-{fl}/ratchet.txt').read().splitlines()]
diff = [(a, b) for a, b in zip(dr, fr) if a != b]
print()
if not diff and len(dr) == len(fr):
    print(f'`ratchet_local.py` (CI\'s Gate A / Gate B / ratchet): identical line for line on both arms ({len(dr)} lines).')
else:
    print('`ratchet_local.py` (CI\'s Gate A / Gate B / ratchet): the lines that differ, develop then fix:')
    print('```')
    for a, b in diff:
        print('dev:', a.strip()); print('fix:', b.strip())
    if len(dr) != len(fr):
        print(f'(line counts differ: {len(dr)} vs {len(fr)})')
    print('```')
for scope, title in (('all', 'all 26'), ('builtins', 'builtins'), ('micro', 'micro')):
    dc, fc = tables[scope]
    print(f'\n<details><summary>Per-case diff_pct ({title}), develop {dsha} vs fix {fsha}</summary>\n')
    print('| case | develop | fix |  |\n|---|---|---|---|')
    for k in sorted(dc):
        moved = '' if (dc[k] is not None and fc.get(k) is not None and abs(dc[k] - fc[k]) < 5e-5) else 'moved'
        print(f'| {k} | {fmt(dc[k])} | {fmt(fc.get(k))} | {moved} |')
    print('\n</details>')
