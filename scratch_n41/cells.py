#!/usr/bin/env python3
"""Print grid container + cell border boxes from a parity-capture layout.json."""
import json
import sys

lay = json.load(open(sys.argv[1]))


def walk(n, depth=0):
    yield n, depth
    for c in n.get('children', []):
        yield from walk(c, depth + 1)


for n, d in walk(lay['root']):
    sel = n.get('selector') or ''
    if 'grid' in sel or 'cell' in sel or 'feature' in sel:
        bb = n['border_box']
        print('%s%-60s x=%7.1f y=%7.1f w=%6.1f h=%5.1f' % ('  ' * d, sel[-58:], bb['x'], bb['y'], bb['width'], bb['height']))
