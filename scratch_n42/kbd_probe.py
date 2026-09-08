#!/usr/bin/env python3
"""Compare new_tab's kbd / shortcut-label / shortcut boxes (RustKit capture) with Chrome rects."""
import json

lay = json.load(open('parity-baseline/captures/new_tab/layout.json'))
ch = json.load(open('baselines/chrome-148/builtins/new_tab/layout-rects.json'))
cm = {e['selector']: e['rect'] for e in ch['elements']}


def walk(n):
    yield n
    for c in n.get('children', []):
        yield from walk(c)


shown = 0
for n in walk(lay['root']):
    s = n.get('selector') or ''
    if s.endswith('kbd') or 'shortcut-label' in s or s.endswith('div.shortcut:nth-of-type(1)'):
        bb = n['border_box']
        p = n['padding']
        cr = cm.get(s)
        crs = None
        if cr:
            crs = (round(cr['left'], 1), round(cr['top'], 1), round(cr['width'], 1), round(cr['height'], 1))
        rk = (round(bb['x'], 1), round(bb['y'], 1), round(bb['width'], 1), round(bb['height'], 1))
        print(n['type'], s[-58:], 'rk', rk, 'pad', p['left'], p['right'], 'chrome', crs)
        shown += 1
        if shown > 8:
            break
