#!/usr/bin/env python3
"""pngcrop.py IN.png OUT.png X0 Y0 X1 Y1 [SCALE] — crop a region of an 8-bit RGB/RGBA PNG (reuses swatches.py's loader)."""
import sys, os, zlib, struct
sys.path.insert(0, os.path.dirname(__file__))
from swatches import load  # noqa: E402

src, out = sys.argv[1], sys.argv[2]
x0, y0, x1, y1 = map(int, sys.argv[3:7])
scale = int(sys.argv[7]) if len(sys.argv) > 7 else 4
w, h, ch, raw = load(src)
rows = []
for y in range(y0, y1):
    line = bytearray()
    for x in range(x0, x1):
        o = (y * w + x) * ch
        line += raw[o:o+3] * scale
    for _ in range(scale):
        rows.append(b'\x00' + bytes(line))
cw, chh = (x1 - x0) * scale, (y1 - y0) * scale
def chunk(t, b):
    return struct.pack('>I', len(b)) + t + b + struct.pack('>I', zlib.crc32(t + b) & 0xffffffff)
png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', cw, chh, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(b''.join(rows))) + chunk(b'IEND', b'')
open(out, 'wb').write(png)
print(out, (cw, chh))
