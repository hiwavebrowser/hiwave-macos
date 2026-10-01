#!/usr/bin/env python3
"""probe.py FRAME.ppm — for each 60px row band (10px margin rows), report the bbox of non-background
(#eef / #fff) pixels and the dominant colors, so a variant repro reads as a table."""
import sys
from collections import Counter

from PIL import Image

im = Image.open(sys.argv[1]).convert('RGB')
w, h = im.size
px = im.load()
BG = {(255, 255, 255), (238, 238, 255)}
row_h = 70  # 60px row + 10px margin
for i in range(h // row_h):
    y0 = i * row_h
    y1 = min(h, y0 + row_h)
    cnt = Counter()
    bbox = None
    for y in range(y0, y1):
        for x in range(w):
            c = px[x, y]
            if c in BG:
                continue
            cnt[c] += 1
            if bbox is None:
                bbox = [x, y, x, y]
            else:
                bbox[0] = min(bbox[0], x)
                bbox[1] = min(bbox[1], y)
                bbox[2] = max(bbox[2], x)
                bbox[3] = max(bbox[3], y)
    top = ', '.join(f'#{r:02x}{g:02x}{b:02x}x{n}' for (r, g, b), n in cnt.most_common(3))
    print(f'row {i + 1}: bbox={bbox} ink={sum(cnt.values())} {top}')
