#!/usr/bin/env python3
"""Diff two layout.json dumps box-by-box (tree position path)."""
import json, sys


def walk(n, path, out):
    bb = n.get("border_box") or n.get("rect")
    tag = n.get("tag") or n.get("type") or ""
    ident = n.get("id") or n.get("class") or ""
    txt = (n.get("text") or "")[:20]
    out.append((path, tag, ident, txt, bb))
    for i, c in enumerate(n.get("children", [])):
        walk(c, f"{path}/{i}", out)


a, b = [], []
walk(json.load(open(sys.argv[1]))["root"], "", a)
walk(json.load(open(sys.argv[2]))["root"], "", b)
bm = {p: r for p, *r in b}
for p, tag, ident, txt, bb in a:
    o = bm.get(p)
    if o is None:
        print("missing in B", p, tag, ident, txt)
        continue
    bb2 = o[3]
    if bb and bb2 and any(abs(bb[k] - bb2[k]) > 0.25 for k in ("x", "y", "width", "height")):
        f = lambda r: f"{r['x']:.1f},{r['y']:.1f} {r['width']:.1f}x{r['height']:.1f}"
        print(f"{p:24} {tag:6} {str(ident)[:24]:24} {txt:20} A {f(bb)}  B {f(bb2)}")
