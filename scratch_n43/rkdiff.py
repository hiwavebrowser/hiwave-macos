#!/usr/bin/env python3
"""Diff two RustKit layout.json captures (before vs after) by selector: border-box deltas > 0.5px."""
import json
import sys

a = json.load(open(sys.argv[1]))
b = json.load(open(sys.argv[2]))
n = int(sys.argv[3]) if len(sys.argv) > 3 else 40


def walk(node):
    yield node
    for c in node.get('children', []):
        yield from walk(c)


def index(lay):
    m = {}
    for node in walk(lay['root']):
        sel = node.get('selector')
        if sel and sel not in m:
            m[sel] = node['border_box']
    return m


ma, mb = index(a), index(b)
rows = []
for sel, ba in ma.items():
    bb = mb.get(sel)
    if bb is None:
        rows.append((999, sel, 'gone'))
        continue
    d = [bb[k] - ba[k] for k in ('x', 'y', 'width', 'height')]
    if max(abs(v) for v in d) > 0.5:
        rows.append((sum(abs(v) for v in d), sel, d))
for sel in mb:
    if sel not in ma:
        rows.append((999, sel, 'new'))
rows.sort(key=lambda r: -r[0])
print(f'{"selector":86s} {"dx":>7s} {"dy":>7s} {"dw":>7s} {"dh":>7s}')
for _, sel, d in rows[:n]:
    if isinstance(d, str):
        print(f'{sel[-86:]:86s} {d}')
    else:
        print(f'{sel[-86:]:86s} {d[0]:7.1f} {d[1]:7.1f} {d[2]:7.1f} {d[3]:7.1f}')
print('changed:', len(rows), 'of', len(ma))
