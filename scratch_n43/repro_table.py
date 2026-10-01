#!/usr/bin/env python3
"""Repro A/B table: pinned-Chrome layout-rects.json vs RustKit repro layout.json (all elements, y/h)."""
import json
import sys

chrome = json.load(open(sys.argv[1]))
lay = json.load(open(sys.argv[2]))


def walk(node):
    yield node
    for c in node.get('children', []):
        yield from walk(c)


rmap = {}
for node in walk(lay['root']):
    sel = node.get('selector')
    if sel and sel not in rmap:
        rmap[sel] = node

print(f'{"selector":66s} {"c.y":>7s} {"c.h":>6s} {"r.y":>7s} {"r.h":>6s} {"dy":>6s} {"dh":>6s}')
bad = 0
for e in chrome['elements']:
    sel = e['selector']
    cr = e['rect']
    r = rmap.get(sel)
    if r is None:
        print(f'{sel[-66:]:66s} {cr["top"]:7.1f} {cr["height"]:6.1f}   (no rustkit join)')
        continue
    bb = r['border_box']
    dy = bb['y'] - cr['top']
    dh = bb['height'] - cr['height']
    flag = ' <' if abs(dy) > 1.5 or abs(dh) > 1.5 else ''
    bad += bool(flag)
    print(f'{sel[-66:]:66s} {cr["top"]:7.1f} {cr["height"]:6.1f} {bb["y"]:7.1f} {bb["height"]:6.1f} {dy:6.1f} {dh:6.1f}{flag}')
print('elements off by >1.5px:', bad)
