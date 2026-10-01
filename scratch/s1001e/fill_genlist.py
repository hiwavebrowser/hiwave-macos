"""Fill s1001e/pr-body-genlist.md from two campaign arms.
usage: fill_genlist.py <dev-arm> <fix-arm> <base-sha> <head-sha>  -> s1001e/pr-body-genlist-final.md"""
import json, sys
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch'
dev, fix, base, head = sys.argv[1:5]


def load(arm, scope):
    j = json.load(open(f'{HUB}/s0929e/camp-{arm}/{scope}.json'))
    rs = j['results']
    return j.get('timestamp', '')[:19], {r['case_id']: r['diff_pct'] for r in rs}, sum(1 for r in rs if r.get('passed'))


summary, tables = [], {}
for scope in ('all', 'builtins', 'micro'):
    ta, a, pa = load(dev, scope)
    tb, b, pb = load(fix, scope)
    same = sum(1 for k in a if a[k] == b.get(k))
    summary.append(f'{scope + ":":9} develop {ta} {pa:2}/{len(a):<2} avg {sum(a.values())/len(a):.4f} | '
                   f'fix {tb} {pb:2}/{len(b):<2} avg {sum(b.values())/len(b):.4f} | {same}/{len(a)} identical')
    tables[scope] = '\n'.join(
        f'| {k} | {a[k]:.4f} | {b[k]:.4f} |' + ('' if a[k] == b[k] else ' moved') for k in sorted(a))
ra = open(f'{HUB}/s0929e/camp-{dev}/ratchet.txt').read().splitlines()
rb = open(f'{HUB}/s0929e/camp-{fix}/ratchet.txt').read().splitlines()
moved = [(x.strip(), y.strip()) for x, y in zip(ra, rb) if x != y]
if not moved and len(ra) == len(rb):
    ratchet = '`ratchet_local.py` (CI\'s Gate A / Gate B / ratchet): output identical on both arms, line for line.'
else:
    ratchet = ('`ratchet_local.py` (CI\'s Gate A / Gate B / ratchet): the lines that differ between the arms, '
               'develop then fix:\n\n```\n' + '\n'.join(f'dev: {x}\nfix: {y}' for x, y in moved) + '\n```')
s = open(f'{HUB}/s1001e/pr-body-genlist.md').read()
for key, val in (('__SUMMARY__', '\n'.join(summary)), ('__RATCHET__', ratchet), ('__TABLE_ALL__', tables['all']),
                 ('__TABLE_REST__', tables['builtins'] + '\n' + tables['micro']), ('__BASE__', base), ('__HEAD__', head)):
    assert key in s, key
    s = s.replace(key, val)
assert '__' not in s, [w for w in s.split() if '__' in w]
open(f'{HUB}/s1001e/pr-body-genlist-final.md', 'w').write(s)
print('\n'.join(summary))
print(ratchet[:1500])
