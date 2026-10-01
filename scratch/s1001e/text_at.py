"""List the text ops of a display-list dump whose y is within [y0, y1].
usage: text_at.py <dl.json> <y0> <y1>"""
import json, sys
j = json.load(open(sys.argv[1]))
cmds = j.get('commands') if isinstance(j, dict) else j
y0, y1 = float(sys.argv[2]), float(sys.argv[3])
for c in cmds:
    if c.get('op') == 'text' and y0 <= c.get('y', -1) <= y1:
        adv = c.get('advances') or []
        print(repr(c['text'][:60]), 'x', round(c['x'], 2), 'y', round(c['y'], 2), 'size', c['font_size'],
              'family', repr(c['font_family'][:50]), 'weight', c['font_weight'], 'style', c['font_style'],
              'run', c.get('run'), 'width', round(sum(adv), 2))
