#!/usr/bin/env python3
"""Reduce Gate A's failures to their ROOT boxes.

NOT A RECEIPT. A displaced parent hands its offset to every descendant, so a
raw failure count is dominated by inheritance: `settings`' 186 attributable `y`
failures are not 186 defects. This asks, per failing box and per axis, whether
every ANCESTOR of that box is green on the same axis. If one is not, the box is
a CHILD of a defect and is not a unit. If all are, the box is a root and the
defect is in how ITS geometry is computed.

Positional nuance that is load-bearing: `x`/`y` inherit down the tree (a parent
moved 8px moves every child 8px) while `width`/`height` do not inherit in the
same way — a parent sized wrong does not necessarily resize a child. Both are
reported, and the `x`/`y` roots are the ones to trust as units.

    python3 trench/tools/n71_root_defects.py <gate-a.json> [--case CASE]
"""
import collections
import json
import sys

AXES = ("x", "y", "width", "height")


def main():
    path = sys.argv[1]
    only = None
    if "--case" in sys.argv:
        only = sys.argv[sys.argv.index("--case") + 1]
    doc = json.load(open(path))

    print("n71 root defects — NOT A RECEIPT")
    rollup = collections.Counter()
    for case in doc["cases"]:
        cid = case["case_id"]
        if only and cid != only:
            continue
        failed = collections.defaultdict(set)  # axis -> set of paths
        info = {}
        for f in case.get("failures", []):
            if f.get("axis") not in AXES:
                continue
            failed[f["axis"]].add(f["path"])
            info[(f["axis"], f["path"])] = f

        roots = collections.defaultdict(list)
        for axis, paths in failed.items():
            for p in paths:
                parts = p.split(".")
                # every strict ancestor path, root-first
                anc = [".".join(parts[:i]) for i in range(1, len(parts))]
                if any(a in paths for a in anc):
                    continue
                roots[axis].append(info[(axis, p)])

        total_roots = sum(len(v) for v in roots.values())
        if not total_roots and not only:
            continue
        tot = sum(len(v) for v in failed.values())
        print(f"\n=== {cid}: {tot} failures -> {total_roots} ROOTS "
              f"({'/'.join(f'{a}:{len(roots[a])}' for a in AXES)})")
        rollup[cid] = total_roots
        for axis in AXES:
            for f in sorted(roots[axis], key=lambda f: -abs(f["delta"] or 0))[:12 if only else 6]:
                sel = f.get("selector") or "(anonymous)"
                print(f"  {axis:>6} d={f['delta']:+9.3f}  exp={f['expected']:9.3f} "
                      f"act={f['actual']:9.3f}  {f['path']:16} {sel}")

    if not only:
        print("\nroot count per case, ranked")
        for cid, n in rollup.most_common():
            print(f"  {cid:26}{n:>5}")
        print(f"  {'TOTAL':26}{sum(rollup.values()):>5}")


if __name__ == "__main__":
    main()
