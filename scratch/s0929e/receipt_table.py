"""usage: python3 receipt_table.py <devlabel> <fixlabel> <out.md> — compare camp-<label>/ dirs."""
import json, sys
dev, fix, outp = sys.argv[1:4]
out = []
for scope in ('all', 'builtins'):
    A = json.load(open(f'camp-{dev}/{scope}.json'))
    B = json.load(open(f'camp-{fix}/{scope}.json'))
    a = {c['case_id']: c['diff_pct'] for c in A['results']}
    b = {c['case_id']: c['diff_pct'] for c in B['results']}
    moved = [k for k in a if abs(a[k] - b.get(k, -1)) > 1e-9]
    print(scope, 'dev ts', A['timestamp'], 'fix ts', B['timestamp'], len(a), 'passed', A['passed'], B['passed'],
          'avg %.3f %.3f' % (sum(a.values()) / len(a), sum(b.values()) / len(b)), 'moved', moved)
    out.append((scope, a, b))
with open(outp, 'w') as f:
    for scope, a, b in out:
        f.write(f'<details><summary>{scope}: per-case diff_pct ({dev} vs {fix})</summary>\n\n')
        f.write(f'| case | {dev} | {fix} |\n|---|---|---|\n')
        for k in a:
            f.write(f'| {k} | {a[k]:.2f}% | {b[k]:.2f}% |\n')
        f.write('\n</details>\n\n')
