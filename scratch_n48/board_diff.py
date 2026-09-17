#!/usr/bin/env python3
"""board_diff.py BEFORE.json AFTER.json — per-case campaign board deltas + averages."""
import json
import sys


def cases(d):
    r = d.get('results') or d.get('cases') or d
    if isinstance(r, list):
        return {x.get('case_id') or x.get('case') or x.get('name'): x for x in r}
    return r


def pct(x):
    if isinstance(x, dict):
        if 'pixel' in x:
            return x['pixel']['diffPercent']
        return x.get('diff_percent', x.get('diff'))
    return x


a = json.load(open(sys.argv[1]))
b = json.load(open(sys.argv[2]))
ca, cb = cases(a), cases(b)
tot_a = tot_b = 0.0
moved = 0
for k in ca:
    pa, pb = pct(ca[k]), pct(cb[k])
    tot_a += pa
    tot_b += pb
    if abs(pa - pb) > 1e-9:
        moved += 1
        print(f"{k:28s} {pa:.4f} -> {pb:.4f}  ({pb - pa:+.4f})")
n = len(ca)
print(f"avg {tot_a / n:.4f} -> {tot_b / n:.4f}; moved {moved}/{n}, byte-flat {n - moved}/{n}")
