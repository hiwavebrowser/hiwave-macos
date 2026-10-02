#!/usr/bin/env python3
"""Split the box-tree walk's samples by what the innermost walk frame called.

    prof_walk_split.py <sample report> [<report> ...] [--walk NAME] [--depth 2] [--top 40]

The walk recurses, so inclusive counts (cascade_prof_sum.py) cannot say how
much of it is style and how much is box building. This takes every stack's
self samples, cuts the stack at its innermost walk frame and charges them to
the next `depth` frames below it ("(self)" when the walk frame is the leaf).
Several reports are pooled. Frames outside the walk are not counted.
"""
import re
import sys
from collections import defaultdict

args = sys.argv[1:]


def opt(name, default):
    if name in args:
        i = args.index(name)
        v = args[i + 1]
        del args[i:i + 2]
        return v
    return default


WALK = opt("--walk", "build_layout_from_parent_style_and_path")
DEPTH = int(opt("--depth", "2"))
TOP = int(opt("--top", "40"))
LINE = re.compile(r"^([\s+!:|]*)(\d+)\s+(.+?)\s+\(in ")


def clean(name):
    name = re.sub(r"::h[0-9a-f]{16}$", "", name)
    name = name.replace("$LT$", "<").replace("$GT$", ">").replace("$u20$", " ").replace("..", "::")
    return name if len(name) < 90 else name[:87] + "..."


split = defaultdict(int)
total = 0
for path in args:
    nodes = []  # [depth, count, name, children count]
    in_graph = False
    for raw in open(path, errors="replace"):
        if raw.startswith("Call graph:"):
            in_graph = True
            continue
        if in_graph and raw.strip().startswith("Total number in stack"):
            break
        m = LINE.match(raw) if in_graph else None
        if m:
            nodes.append([len(m.group(1)), int(m.group(2)), clean(m.group(3)), 0])
    stack = []
    for node in nodes:
        while stack and stack[-1][0] >= node[0]:
            stack.pop()
        if stack:
            stack[-1][3] += node[1]
        stack.append(node)
        node.append([n[2] for n in stack])
    for depth, count, name, kids, names in nodes:
        own = count - kids
        if own <= 0:
            continue
        cut = max((i for i, n in enumerate(names) if WALK in n), default=None)
        if cut is None:
            continue
        below = names[cut + 1:cut + 1 + DEPTH] or ["(self)"]
        split[" > ".join(below)] += own
        total += own

print("%d self samples under the innermost %s, %d report(s)" % (total, WALK, len(args)))
for key, n in sorted(split.items(), key=lambda kv: -kv[1])[:TOP]:
    print("%6.1f%% %6d  %s" % (100.0 * n / max(total, 1), n, key))
