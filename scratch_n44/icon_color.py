#!/usr/bin/env python3
"""icon_color.py IMAGE [IMAGE...] — count pixels near the repro's icon color (#6b7280) and near black,
and print the bbox of the icon-colored pixels. PPM or PNG. Used as the currentColor receipt on
parity-tests/repro/inline-svg.html (Chrome baseline in scratch_n38/chrome-inline-svg/baseline.png)."""
import sys

from PIL import Image

TARGET = (0x6B, 0x72, 0x80)


def near(p, t, tol):
    return all(abs(a - b) <= tol for a, b in zip(p, t))


for path in sys.argv[1:]:
    im = Image.open(path).convert('RGB')
    w, h = im.size
    px = im.load()
    gray, black = 0, 0
    xs, ys = [], []
    for y in range(h):
        for x in range(w):
            p = px[x, y]
            if near(p, TARGET, 28):
                gray += 1
                xs.append(x)
                ys.append(y)
            elif near(p, (0, 0, 0), 60):
                black += 1
    bbox = (min(xs), min(ys), max(xs), max(ys)) if xs else None
    print(f'{path}: {w}x{h}  icon-gray px={gray} bbox={bbox}  near-black px={black}')
