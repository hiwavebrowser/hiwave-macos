#!/usr/bin/env python3
"""Which engine frames sit above a leaf function in a macOS `sample` call graph.

usage: prof_parents.py <report> <leaf_substring> [frame_filter=rustkit]
For every line whose frame contains <leaf_substring>, attribute its sample
count to the nearest enclosing frame containing <frame_filter> (the innermost
engine frame), without counting nested occurrences of the leaf twice.
"""
import collections
import re
import sys

LINE = re.compile(r"^([\s+!:|]*)(\d+)\s+(.*)$")


def main():
    report, leaf = sys.argv[1], sys.argv[2]
    filt = sys.argv[3] if len(sys.argv) > 3 else "rustkit"
    stack = []  # (depth, name)
    totals = collections.Counter()
    in_leaf_depth = None
    for raw in open(report, errors="replace"):
        m = LINE.match(raw.rstrip("\n"))
        if not m:
            continue
        depth, count, name = len(m.group(1)), int(m.group(2)), m.group(3)
        while stack and stack[-1][0] >= depth:
            stack.pop()
        if in_leaf_depth is not None and depth <= in_leaf_depth:
            in_leaf_depth = None
        if leaf in name and in_leaf_depth is None:
            parent = next((n for _, n in reversed(stack) if filt in n), "?")
            parent = re.sub(r"\s+\(in .*$", "", parent)[:110]
            totals[parent] += count
            in_leaf_depth = depth
        stack.append((depth, name))
    total = sum(totals.values())
    print(f"{leaf}: {total} samples")
    for p, c in totals.most_common(25):
        print(f"{c:7d}  {p}")


if __name__ == "__main__":
    main()
