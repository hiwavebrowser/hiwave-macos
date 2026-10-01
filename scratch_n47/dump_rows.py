#!/usr/bin/env python3
"""dump_rows.py LAYOUT.json Y0 Y1 — every box whose y is within [Y0, Y1], with tag/class and rect."""
import json
import sys

d = json.load(open(sys.argv[1]))
y0, y1 = float(sys.argv[2]), float(sys.argv[3])


def rect_of(n):
    r = n.get('rect') or n.get('border_box') or n
    return [r.get(k) for k in ('x', 'y', 'width', 'height')]


def walk(n, depth=0):
    if not isinstance(n, dict):
        return
    x, y, w, h = rect_of(n)
    tag = n.get('tag') or n.get('element') or n.get('name') or '?'
    cls = n.get('class') or n.get('classes') or n.get('id') or ''
    if y is not None and y0 <= float(y) <= y1:
        print('  ' * depth + f'{tag} {cls} x={x} y={y} w={w} h={h}')
    for c in n.get('children', []) or []:
        walk(c, depth + 1)


root = d.get('root', d) if isinstance(d, dict) else {'children': d}
if isinstance(root, dict):
    print('keys:', list(root.keys())[:12])
walk(root)
