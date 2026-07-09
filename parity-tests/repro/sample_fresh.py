#!/usr/bin/env python3
"""Sample chrome baseline vs a freshly captured frame at the same coords."""
import sys
from PIL import Image

base_path = sys.argv[1]
fresh_path = sys.argv[2]
base = Image.open(base_path).convert('RGB')
rk = Image.open(fresh_path).convert('RGB')
print('chrome size', base.size, '| rustkit size', rk.size)

pts = [
    ('top-left    ', (5, 5)),
    ('h1 area     ', (40, 35)),
    ('below h1    ', (40, 80)),
    ('mid page    ', (100, 170)),
    ('box row 1   ', (100, 200)),
    ('box row 2   ', (330, 200)),
    ('lower left  ', (100, 350)),
    ('lower mid   ', (100, 460)),
    ('right side  ', (560, 250)),
]
for name, (x, y) in pts:
    if x >= min(base.size[0], rk.size[0]) or y >= min(base.size[1], rk.size[1]):
        continue
    c = base.getpixel((x, y))
    r = rk.getpixel((x, y))
    d = tuple(abs(a - b) for a, b in zip(c, r))
    flag = '' if d == (0, 0, 0) else '   <-- DIFF'
    print(f'{name} ({x:3},{y:3}): chrome={c} rustkit={r}{flag}')
