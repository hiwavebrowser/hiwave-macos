#!/usr/bin/env python3
"""ink.py FRAME.ppm — connected bboxes of non-background ink, grouped by color (bg = #fff / #eef)."""
import sys
from collections import defaultdict

from PIL import Image

im = Image.open(sys.argv[1]).convert('RGB')
w, h = im.size
px = im.load()
BG = {(255, 255, 255), (238, 238, 255)}
boxes = defaultdict(lambda: [w, h, -1, -1, 0])
for y in range(h):
    for x in range(w):
        c = px[x, y]
        if c in BG:
            continue
        b = boxes[c]
        b[0] = min(b[0], x)
        b[1] = min(b[1], y)
        b[2] = max(b[2], x)
        b[3] = max(b[3], y)
        b[4] += 1
for c, b in sorted(boxes.items(), key=lambda kv: -kv[1][4])[:12]:
    print(f'#{c[0]:02x}{c[1]:02x}{c[2]:02x}: n={b[4]} bbox x {b[0]}..{b[2]} y {b[1]}..{b[3]}')
