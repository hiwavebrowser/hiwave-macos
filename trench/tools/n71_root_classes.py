#!/usr/bin/env python3
"""Group Gate A ROOT defects into CLASSES by their repeated delta.

NOT A RECEIPT. 839 roots is not 839 defects either. When thirty `a` elements
each read height -2.520 to the thousandth, that is one rule computing one
quantity wrongly thirty times, and a board that ranks by count makes it look
thirty times more important than a single 300px displacement. This groups roots
by (case, axis, rounded delta) and ranks by the GROUP, printing one exemplar.

A class whose delta is identical to 3 decimal places across many unrelated
elements is a constant the engine is adding or dropping. A class whose deltas
all differ is a per-element computation, which is a different kind of unit.

    python3 trench/tools/n71_root_classes.py <gate-a.json>
"""
import collections
import json
import sys

AXES = ("x", "y", "width", "height")


def main():
    doc = json.load(open(sys.argv[1]))
    classes = collections.defaultdict(list)
    for case in doc["cases"]:
        cid = case["case_id"]
        failed = collections.defaultdict(set)
        info = {}
        for f in case.get("failures", []):
            if f.get("axis") not in AXES:
                continue
            failed[f["axis"]].add(f["path"])
            info[(f["axis"], f["path"])] = f
        for axis, paths in failed.items():
            for p in paths:
                parts = p.split(".")
                if any(".".join(parts[:i]) in paths for i in range(1, len(parts))):
                    continue
                f = info[(axis, p)]
                classes[(cid, axis, round(f["delta"], 3))].append(f)

    print("n71 root defect CLASSES — NOT A RECEIPT")
    print(f"{'case':22}{'axis':>7}{'delta':>11}{'n':>5}  exemplar selector")
    ranked = sorted(classes.items(), key=lambda kv: (-len(kv[1]), -abs(kv[0][2])))
    for (cid, axis, delta), fs in ranked[:30]:
        sel = fs[0].get("selector") or "(anonymous)"
        print(f"{cid:22}{axis:>7}{delta:>+11.3f}{len(fs):>5}  {sel[-66:]}")

    print(f"\n{len(classes)} classes over {sum(len(v) for v in classes.values())} roots")
    print("\nclasses with n>=5, grouped by delta across ALL cases "
          "(a shared constant is one rule, not one per page)")
    byd = collections.defaultdict(lambda: collections.Counter())
    for (cid, axis, delta), fs in classes.items():
        if len(fs) >= 5:
            byd[(axis, delta)][cid] = len(fs)
    for (axis, delta), c in sorted(byd.items(), key=lambda kv: -sum(kv[1].values()))[:15]:
        spread = " ".join(f"{k}:{v}" for k, v in c.most_common())
        print(f"  {axis:>6}{delta:>+10.3f}  n={sum(c.values()):<4} {spread}")


if __name__ == "__main__":
    main()
