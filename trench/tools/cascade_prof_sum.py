#!/usr/bin/env python3
"""Inclusive sample counts per function from a macOS `sample` call graph.

usage: cascade-tools-s5-sum.py <report> [root_substring]
Counts each frame once per stack (recursion-safe), relative to the root frame.
"""
import re
import sys
from collections import defaultdict

path = sys.argv[1]
root = sys.argv[2] if len(sys.argv) > 2 else "build_layout_from_document"
line_re = re.compile(r"^([\s+!:|]*)(\d+)\s+(.+?)\s+\(in ")

stack = []  # (depth, name)
incl = defaultdict(int)
root_total = 0
in_graph = False
for raw in open(path, errors="replace"):
    if raw.startswith("Call graph:"):
        in_graph = True
        continue
    if in_graph and raw.strip().startswith("Total number in stack"):
        break
    if not in_graph:
        continue
    m = line_re.match(raw)
    if not m:
        continue
    depth = len(m.group(1))
    count = int(m.group(2))
    name = re.sub(r"::h[0-9a-f]{16}$", "", m.group(3))
    name = re.sub(r"<.*?>", "", name)
    while stack and stack[-1][0] >= depth:
        stack.pop()
    names_above = [n for _, n in stack]
    under_root = any(root in n for n in names_above) or root in name
    stack.append((depth, name))
    if not under_root:
        continue
    if root in name and not any(root in n for n in names_above):
        root_total += count
    if name not in names_above:
        incl[name] += count

print(f"root {root}: {root_total} samples")
for name, c in sorted(incl.items(), key=lambda kv: -kv[1])[:45]:
    short = name if len(name) < 110 else name[:107] + "..."
    print(f"{100.0 * c / max(root_total, 1):6.1f}%  {c:6d}  {short}")
