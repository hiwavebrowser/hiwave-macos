#!/usr/bin/env python3
"""Locate each expected swatch color in both Chrome baseline and RustKit capture."""
import sys
import numpy as np
from PIL import Image

case = sys.argv[1] if len(sys.argv) > 1 else 'bg-solid'
scope = sys.argv[2] if len(sys.argv) > 2 else 'micro'

targets = {
    'red        ': (255, 0, 0),
    'coral      ': (255, 127, 80),
    'hex-blue   ': (52, 152, 219),
    'rgb-green  ': (46, 204, 113),
    'rgba-purple': (185, 136, 194),   # rgba(155,89,182,.7) over #f5f5f5
    'hsl-green  ': (64, 191, 64),
    'hsla-blue  ': (159, 159, 223),   # hsla(240,50%,50%,.5) over #f5f5f5
}

for label, path in [('chrome ', f'baselines/chrome-148/{scope}/{case}/baseline.png'),
                    ('rustkit', f'parity-baseline/captures/{case}/frame.ppm')]:
    a = np.asarray(Image.open(path).convert('RGB'), dtype=np.int16)
    print(f'--- {label} {a.shape[1]}x{a.shape[0]}')
    for name, (r, g, b) in targets.items():
        m = (abs(a[:, :, 0] - r) < 14) & (abs(a[:, :, 1] - g) < 14) & (abs(a[:, :, 2] - b) < 14)
        n = int(m.sum())
        if n:
            ys, xs = m.nonzero()
            print(f'  {name} {n:6d}px  bbox x{xs.min()}-{xs.max()} y{ys.min()}-{ys.max()}')
        else:
            print(f'  {name}      0px  MISSING')
