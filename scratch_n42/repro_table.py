#!/usr/bin/env python3
"""Repro geometry table: RustKit layout.json vs Chrome layout-rects.json for the n42 repro (all elements)."""
import json
import sys

rk_path = sys.argv[1] if len(sys.argv) > 1 else 'parity-tests/repro/flex-item-padding-column-basis.layout.json'
ch_path = sys.argv[2] if len(sys.argv) > 2 else 'scratch_n42/chrome-repro/layout-rects.json'
chrome = json.load(open(ch_path))
lay = json.load(open(rk_path))


def walk(node):
    yield node
    for c in node.get('children', []):
        yield from walk(c)


rmap = {}
for node in walk(lay['root']):
    sel = node.get('selector')
    if sel and sel not in rmap:
        rmap[sel] = node['border_box']

print(f'{"selector":60s} {"cx":>7s} {"cy":>7s} {"cw":>7s} {"ch":>7s} | {"dx":>6s} {"dy":>6s} {"dw":>6s} {"dh":>6s}')
bad = 0
for e in chrome['elements']:
    sel = e['selector']
    cr = e['rect']
    bb = rmap.get(sel)
    if bb is None:
        print(f'{sel[-60:]:60s}  (no RustKit join)')
        continue
    dx = bb['x'] - cr['left']
    dy = bb['y'] - cr['top']
    dw = bb['width'] - cr['width']
    dh = bb['height'] - cr['height']
    flag = '' if max(abs(dx), abs(dy), abs(dw), abs(dh)) <= 1.0 else '  <<'
    if flag:
        bad += 1
    print(f'{sel[-60:]:60s} {cr["left"]:7.1f} {cr["top"]:7.1f} {cr["width"]:7.1f} {cr["height"]:7.1f} | {dx:6.1f} {dy:6.1f} {dw:6.1f} {dh:6.1f}{flag}')
print('elements off by >1px:', bad, '/', len(chrome['elements']))
