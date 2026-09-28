#!/usr/bin/env python3
"""usage: frame_moves.py <run-a> <run-b> — per site, % of RustKit pixels that differ (any channel > 8) and the bbox."""
import sys
from pathlib import Path
import numpy as np
from PIL import Image

a, b = Path(sys.argv[1]), Path(sys.argv[2])
for site in sorted(p.name for p in a.iterdir() if p.is_dir()):
    fa, fb = a / site / "rustkit.ppm", b / site / "rustkit.ppm"
    if not (fa.exists() and fb.exists()):
        print(f"{site:10} -")
        continue
    x = np.asarray(Image.open(fa).convert("RGB"), dtype=np.int16)
    y = np.asarray(Image.open(fb).convert("RGB"), dtype=np.int16)
    if x.shape != y.shape:
        print(f"{site:10} size {x.shape} vs {y.shape}")
        continue
    m = (np.abs(x - y).max(axis=2) > 8)
    if not m.any():
        print(f"{site:10} identical")
        continue
    ys, xs = np.nonzero(m)
    print(f"{site:10} {100 * m.mean():6.2f}%  bbox x{xs.min()}-{xs.max()} y{ys.min()}-{ys.max()}")
