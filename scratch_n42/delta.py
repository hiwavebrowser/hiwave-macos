#!/usr/bin/env python3
"""Per-case delta: n39 clean-develop basis (develop 5b89ed8) vs a with-fix board (default: latest parity_test_results.json)."""
import json
import shutil
import sys


def load(p):
    d = json.load(open(p))
    return {c['case_id']: c['pixel']['diffPercent'] for c in d['results']}


src = sys.argv[1] if len(sys.argv) > 1 else 'parity-baseline/parity_test_results.json'
if len(sys.argv) > 2:
    shutil.copy(src, sys.argv[2])
base = load('scratch_n39/board_develop_basis.json')
fix = load(src)
flat = 0
for k in base:
    b, f = base[k], fix.get(k)
    if f is None:
        continue
    if abs(b - f) < 1e-6:
        flat += 1
    else:
        print('%-26s %8.4f -> %8.4f (%+.4fpp)' % (k, b, f, f - b))
print('byte-flat %d / %d' % (flat, len(base)))
print('avg basis %.4f  avg fix %.4f' % (sum(base.values()) / len(base), sum(fix.values()) / len(fix)))
