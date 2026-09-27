#!/usr/bin/env python3
"""Per-case diff_pct table for a PR receipt: campaign_table.py <develop-results.json> <pr-results.json>"""
import json, sys


def cases(path):
    d = json.load(open(path))
    return d.get("timestamp"), {
        (r.get("case_id") or r.get("case") or r.get("name")): r for r in d.get("results", d)
    }


ta, a = cases(sys.argv[1])
tb, b = cases(sys.argv[2])
avg = lambda c: sum(r["diff_pct"] for r in c.values()) / len(c)
print(f"develop run {ta}, PR run {tb}")
print(f"passed {sum(1 for r in b.values() if r.get('passed'))}/{len(b)}, "
      f"avg {avg(b):.3f}% (develop {avg(a):.3f}%)")
print("| case | develop | this PR |\n|---|---|---|")
for k in sorted(b):
    print(f"| {k} | {a[k]['diff_pct']:.2f}% | {b[k]['diff_pct']:.2f}% |")
