#!/usr/bin/env python3
"""Print a campaign receipt table from parity_test_results.json vs a develop column.

usage: receipt.py <results.json>
"""
import json, sys

# #300's receipt (develop-equivalent for fixtures), copied from #302's body.
DEV = {
    "new_tab": 1.5685, "about": 3.7492, "settings": 2.0854, "chrome_rustkit": 1.1234,
    "shelf": 2.8711, "article-typography": 4.7857, "card-grid": 1.2958,
    "css-selectors": 1.4975, "flex-positioning": 0.6793, "form-elements": 0.9875,
    "gradient-backgrounds": 1.0133, "image-gallery": 0.5217, "sticky-scroll": 0.6575,
    "backgrounds": 1.2163, "bg-solid": 0.2106, "bg-pure": 0.0, "combinators": 0.6547,
    "form-controls": 3.2442, "gradients": 0.1412, "gradient-no-radius": 0.5204,
    "gradient-radius-only": 0.6892, "gpu-gradient-regression": 0.3903,
    "images-intrinsic": 0.3267, "pseudo-classes": 0.4341, "rounded-corners": 1.3221,
    "specificity": 0.6015,
}
d = json.load(open(sys.argv[1]))
rows = [(r["case_id"], r["diff_pct"], r["passed"]) for r in d["results"]]
avg = sum(r[1] for r in rows) / len(rows)
same = sum(1 for c, v, _ in rows if abs(v - DEV.get(c, -1)) < 0.00005)
print(f"timestamp {d['timestamp']}  passed {sum(r[2] for r in rows)}/{len(rows)}  "
      f"avg {avg:.4f}  dev avg {sum(DEV.values())/len(DEV):.4f}  identical {same}/{len(rows)}")
print("| case | diff_pct | #300 |\n|---|---|---|")
for c, v, _ in rows:
    print(f"| `{c}` | {v:.4f} | {DEV.get(c, float('nan')):.4f} |")
