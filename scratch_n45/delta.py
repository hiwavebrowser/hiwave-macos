#!/usr/bin/env python3
"""delta.py BEFORE.json AFTER.json — per-case diffPercent delta between two parity_test result files."""
import json
import sys


def load(p):
    d = json.load(open(p))
    cases = d["results"] if isinstance(d, dict) and "results" in d else d
    return {c["case_id"]: c["pixel"]["diffPercent"] for c in cases if c.get("pixel")}


a = load(sys.argv[1])
b = load(sys.argv[2])
flat = 0
for k in sorted(set(a) | set(b)):
    x, y = a.get(k), b.get(k)
    if x is None or y is None:
        print(f"{k:28s} {x} -> {y}")
        continue
    d = y - x
    if abs(d) < 1e-9:
        flat += 1
    print(f"{k:28s} {x:8.4f} -> {y:8.4f}  {d:+.4f}")
print(f"\navg {sum(a.values())/len(a):.4f} -> {sum(b.values())/len(b):.4f}   byte-flat {flat}/{len(b)}")
