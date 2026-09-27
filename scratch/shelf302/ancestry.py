#!/usr/bin/env python3
"""Print the ancestor chain (with rects) of the first text box matching a word.

usage: ancestry.py <layout.json> <word>
"""
import json, sys

root = json.load(open(sys.argv[1]))["root"]
word = sys.argv[2]


def find(n, chain):
    if word in (n.get("text") or ""):
        return chain + [n]
    for c in n.get("children", []):
        r = find(c, chain + [n])
        if r:
            return r
    return None


chain = find(root, []) or []
for depth, n in enumerate(chain):
    bb = n.get("border_box") or n.get("rect") or {}
    keys = {k: n[k] for k in ("tag", "type", "id", "class", "display") if k in n}
    print(f"{depth:2} {bb.get('x', 0):7.1f} {bb.get('y', 0):7.1f} {bb.get('width', 0):7.1f}x{bb.get('height', 0):7.1f} "
          f"{keys} {(n.get('text') or '')[:30]!r} kids={len(n.get('children', []))}")
