#!/usr/bin/env python3
"""Dump body / .container / .shortcuts boxes from a RustKit layout.json (before or after)."""
import json
import sys

lay = json.load(open(sys.argv[1]))


def walk(n, depth=0):
    yield n, depth
    for c in n.get('children', []):
        yield from walk(c, depth + 1)


for n, depth in walk(lay['root']):
    s = n.get('selector') or ''
    if s in ('html > body', 'body > div.container:nth-of-type(2)') or s.endswith('div.shortcuts:nth-of-type(2)') or s.endswith('div.shortcuts-section:nth-of-type(3)') or s.endswith('div.search-container:nth-of-type(2)') or s.endswith('p.tagline') or s.endswith('div.logo-wrapper:nth-of-type(1)') or depth == 0:
        bb = n['border_box']
        cr = n['content_rect']
        m = n['margin']
        p = n['padding']
        print(f"{n['type']:6s} d{depth} {s[-50:]:50s} bb=({bb['x']:.1f},{bb['y']:.1f},{bb['width']:.1f},{bb['height']:.1f}) content_h={cr['height']:.1f} m=({m['top']},{m['bottom']}) p=({p['top']},{p['bottom']})")
