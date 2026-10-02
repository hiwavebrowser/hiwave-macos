#!/usr/bin/env python3
"""prof_children.py <sample report> <frame substring> [top=40]

Direct callees of every instance of a frame in a macOS `sample` call graph,
summed over all instances (a recursive frame's nested instances each count
their own direct callees; the recursive call itself is listed as a callee and
is the only double count). Also prints the frame's self samples (its count
minus its callees'), summed.
"""
import collections
import re
import sys

LINE = re.compile(r"^([\s+!:|]*)(\d+)\s+(.+?)\s+\(in ")
report, frame = sys.argv[1:3]
top = int(sys.argv[3]) if len(sys.argv) > 3 else 40
rows = []
in_graph = False
for raw in open(report, errors="replace"):
    if raw.startswith("Call graph:"):
        in_graph = True
        continue
    if in_graph and raw.strip().startswith("Total number in stack"):
        break
    if not in_graph:
        continue
    m = LINE.match(raw)
    if m:
        rows.append((len(m.group(1)), int(m.group(2)), m.group(3)))

callees = collections.Counter()
self_total = 0
outer_total = 0
stack = []  # depths of enclosing instances of the frame
for i, (depth, count, name) in enumerate(rows):
    while stack and stack[-1] >= depth:
        stack.pop()
    if frame in name:
        if not stack:
            outer_total += count
        kids = 0
        j = i + 1
        child_depth = None
        while j < len(rows) and rows[j][0] > depth:
            if child_depth is None:
                child_depth = rows[j][0]
            if rows[j][0] == child_depth:
                callees[rows[j][2]] += rows[j][1]
                kids += rows[j][1]
            j += 1
        self_total += count - kids
        stack.append(depth)

print("outermost instances: %d samples; self (all instances): %d" % (outer_total, self_total))
for name, n in callees.most_common(top):
    print("%6d  %5.1f%%  %s" % (n, 100.0 * n / max(outer_total, 1), name[:150]))
