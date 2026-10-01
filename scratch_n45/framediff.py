#!/usr/bin/env python3
"""framediff.py A.ppm B.ppm — bbox + count of differing pixels between two P6 frames, plus row histogram."""
import sys
from collections import Counter

def load(p):
    d = open(p, 'rb').read()
    parts = d.split(maxsplit=4)
    return int(parts[1]), int(parts[2]), parts[4]

wa, ha, a = load(sys.argv[1])
wb, hb, b = load(sys.argv[2])
assert (wa, ha) == (wb, hb), (wa, ha, wb, hb)
n = 0; x0 = y0 = 10**9; x1 = y1 = -1
rows = Counter()
samples = []
for y in range(ha):
    for x in range(wa):
        o = (y * wa + x) * 3
        if a[o:o+3] != b[o:o+3]:
            n += 1; rows[y] += 1
            x0 = min(x0, x); x1 = max(x1, x); y0 = min(y0, y); y1 = max(y1, y)
            if len(samples) < 6:
                samples.append((x, y, a[o:o+3].hex(), b[o:o+3].hex()))
print('diff px', n, 'bbox', (x0, y0, x1, y1))
print('rows', sorted(rows.items())[:40])
print('samples (x,y,before,after)', samples)
