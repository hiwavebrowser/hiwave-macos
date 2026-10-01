#!/usr/bin/env python3
"""Case-generic Gate-A geometry table: RustKit capture layout.json vs pinned Chrome layout-rects.json.

usage: ytable.py <group> <case> [--all] [--n N]
  group = builtins|websuite|micro|holdout
Prints (1) the first element in document order whose y diverges > 2px, then
(2) the top-N mismatches inside the Chrome viewport ranked by |dy|+|dx|.
"""
import json
import sys

group, case = sys.argv[1], sys.argv[2]
show_all = '--all' in sys.argv
n = 25
if '--n' in sys.argv:
    n = int(sys.argv[sys.argv.index('--n') + 1])

chrome = json.load(open(f'baselines/chrome-148/{group}/{case}/layout-rects.json'))
vp_h = chrome['viewport']['height'] if isinstance(chrome.get('viewport'), dict) else 600
cmap = {}
corder = []
for e in chrome['elements']:
    cmap[e['selector']] = e['rect']
    corder.append(e['selector'])

lay = json.load(open(f'parity-baseline/captures/{case}/layout.json'))


def walk(node):
    yield node
    for c in node.get('children', []):
        yield from walk(c)


rmap = {}
for node in walk(lay['root']):
    sel = node.get('selector')
    if sel and sel not in rmap:
        rmap[sel] = node['border_box']

rows = []
first = None
matched = 0
for sel in corder:
    cr = cmap[sel]
    bb = rmap.get(sel)
    if bb is None:
        continue
    matched += 1
    if not show_all and cr['top'] > vp_h:
        continue
    dx = bb['x'] - cr['left']
    dy = bb['y'] - cr['top']
    dw = bb['width'] - cr['width']
    dh = bb['height'] - cr['height']
    if abs(dy) > 2 and first is None:
        first = (sel, dx, dy, dw, dh, cr['top'])
    if abs(dy) > 2 or abs(dw) > 2 or abs(dx) > 2 or abs(dh) > 2:
        rows.append((abs(dy) + abs(dx), sel[:78], round(dx, 1), round(dy, 1), round(dw, 1), round(dh, 1), round(cr['top'], 1), round(cr['height'], 1)))

print(f'== {group}/{case}  chrome viewport h={vp_h}  chrome elements={len(corder)}  joined={matched}')
if first:
    print('FIRST dy>2 in document order: %s  dx=%.1f dy=%.1f dw=%.1f dh=%.1f (chrome top %.1f)' % first)
else:
    print('no dy>2 in viewport')
rows.sort(key=lambda r: -r[0])
print(f'{"selector":78s} {"dx":>7s} {"dy":>7s} {"dw":>7s} {"dh":>7s} {"cy":>7s} {"ch":>7s}')
for _, sel, dx, dy, dw, dh, cy, ch in rows[:n]:
    print(f'{sel:78s} {dx:7.1f} {dy:7.1f} {dw:7.1f} {dh:7.1f} {cy:7.1f} {ch:7.1f}')
print('mismatched in viewport:', len(rows))
