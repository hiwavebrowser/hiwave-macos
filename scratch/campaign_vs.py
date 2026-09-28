#!/usr/bin/env python3
"""Campaign receipt: a PR's parity_test_results.json against develop's.

usage: campaign_vs.py <develop results.json> <pr results.json> <out.md>
"""
import json
import sys

a = json.load(open(sys.argv[1]))
b = json.load(open(sys.argv[2]))
dev = {r["case_id"]: r["diff_pct"] for r in a["results"]}
rows = [(r["case_id"], r["diff_pct"], r["passed"]) for r in b["results"]]
same = sum(1 for c, v, _ in rows if abs(v - dev.get(c, -9)) < 5e-5)
avg = sum(v for _, v, _ in rows) / len(rows)
print(f"timestamp {b['timestamp']}  passed {sum(p for *_, p in rows)}/{len(rows)}  "
      f"avg {avg:.4f}  develop avg {sum(dev.values()) / len(dev):.4f}  identical {same}/{len(rows)}")
for c, v, _ in rows:
    if abs(v - dev.get(c, -9)) >= 5e-5:
        print("DIFF", c, dev.get(c), v)
lines = ["| case | this PR | develop |", "|---|---|---|"]
lines += [f"| `{c}` | {v:.4f} | {dev.get(c, float('nan')):.4f} |" for c, v, _ in rows]
open(sys.argv[3], "w").write("\n".join(lines) + "\n")
