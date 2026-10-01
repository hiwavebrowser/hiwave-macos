#!/usr/bin/env python3
"""crop.py IN.ppm OUT.png X0 Y0 X1 Y1 [SCALE] — crop a P6 frame region to PNG (nearest-neighbour upscale)."""
import sys, zlib, struct

src, out = sys.argv[1], sys.argv[2]
x0, y0, x1, y1 = map(int, sys.argv[3:7])
scale = int(sys.argv[7]) if len(sys.argv) > 7 else 4
d = open(src, 'rb').read()
parts = d.split(maxsplit=4)
w, h = int(parts[1]), int(parts[2]); raw = parts[4]
cw, ch = (x1 - x0) * scale, (y1 - y0) * scale
rows = []
for y in range(y0, y1):
    line = bytearray()
    for x in range(x0, x1):
        o = (y * w + x) * 3
        line += raw[o:o+3] * scale
    for _ in range(scale):
        rows.append(b'\x00' + bytes(line))
body = zlib.compress(b''.join(rows))
def chunk(t, b):
    return struct.pack('>I', len(b)) + t + b + struct.pack('>I', zlib.crc32(t + b) & 0xffffffff)
png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', cw, ch, 8, 2, 0, 0, 0)) + chunk(b'IDAT', body) + chunk(b'IEND', b'')
open(out, 'wb').write(png)
print(out, (cw, ch))
