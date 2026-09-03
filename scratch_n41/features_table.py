#!/usr/bin/env python3
"""Rects of the about page's .features grid cells: RustKit layout.json vs pinned Chrome layout-rects.json."""
import json
import sys

lay_path = sys.argv[1] if len(sys.argv) > 1 else 'crates/hiwave-app/src/ui/about.layout.json'
chrome = json.load(open('baselines/chrome-148/builtins/about/layout-rects.json'))
cmap = {e['selector']: e['rect'] for e in chrome['elements']}
lay = json.load(open(lay_path))


def walk(n):
    yield n
    for c in n.get('children', []):
        yield from walk(c)


for n in walk(lay['root']):
    sel = n.get('selector') or ''
    if 'feature' in sel and 'icon' not in sel:
        cr = cmap.get(sel)
        bb = n['border_box']
        if cr:
            print('%-58s rk x=%7.1f y=%7.1f w=%6.1f h=%5.1f | ch x=%7.1f y=%7.1f w=%6.1f h=%5.1f' % (
                sel[:58], bb['x'], bb['y'], bb['width'], bb['height'],
                cr['left'], cr['top'], cr['width'], cr['height']))
