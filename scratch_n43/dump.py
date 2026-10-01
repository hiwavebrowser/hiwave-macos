#!/usr/bin/env python3
"""Side-by-side subtree dump: Chrome layout-rects vs RustKit capture, filtered by selector prefix.
usage: dump.py <group> <case> <selector-substring> [captures-dir]
"""
import json
import sys

group, case, needle = sys.argv[1], sys.argv[2], sys.argv[3]
pos = [a for a in sys.argv[4:] if not a.startswith('--')]
capdir = pos[0] if pos else 'parity-baseline/captures'
chrome = json.load(open(f'baselines/chrome-148/{group}/{case}/layout-rects.json'))
lay = json.load(open(f'{capdir}/{case}/layout.json'))


def walk(node, depth=0):
    yield node, depth
    for c in node.get('children', []):
        yield from walk(c, depth + 1)


rmap = {}
rnodes = []
for node, depth in walk(lay['root']):
    sel = node.get('selector')
    rnodes.append((node, depth))
    if sel and sel not in rmap:
        rmap[sel] = node

print(f'{"selector":70s} {"c.y":>7s} {"c.h":>6s} {"r.y":>7s} {"r.h":>6s} {"dy":>6s} {"dh":>6s}')
for e in chrome['elements']:
    sel = e['selector']
    if needle not in sel:
        continue
    cr = e['rect']
    r = rmap.get(sel)
    if r is None:
        print(f'{sel[-70:]:70s} {cr["top"]:7.1f} {cr["height"]:6.1f}   (no rustkit join)')
        continue
    bb = r['border_box']
    print(f'{sel[-70:]:70s} {cr["top"]:7.1f} {cr["height"]:6.1f} {bb["y"]:7.1f} {bb["height"]:6.1f} {bb["y"]-cr["top"]:6.1f} {bb["height"]-cr["height"]:6.1f}')

if '--tree' in sys.argv:
    # print the rustkit subtree under the first joined node matching needle, including anonymous/text boxes
    for node, depth in rnodes:
        sel = node.get('selector') or ''
        if needle in sel:
            for n2, d2 in walk(node):
                bb = n2.get('border_box') or n2.get('rect') or n2.get('content_rect') or {}
                keys = {k: n2[k] for k in n2 if k not in ('children', 'border_box', 'selector', 'padding_box', 'margin_box', 'content_rect')}
                geo = f"y={bb.get('y', 0):.1f} h={bb.get('height', 0):.1f} x={bb.get('x', 0):.1f} w={bb.get('width', 0):.1f}" if bb else '(no rect)'
                print('  ' * d2, (n2.get('selector') or n2.get('type') or '')[-40:], geo, str(keys)[:300])
            break
