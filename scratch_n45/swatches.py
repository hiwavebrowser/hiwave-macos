#!/usr/bin/env python3
"""swatches.py IMAGE — read the centre pixel of each 40x40 swatch in the unknown-pseudo-class repro
(rows A..E at y=0,40,..; swatches at x=0,40,..). IMAGE is a .ppm (P6) or .png."""
import sys, struct, zlib

def load(path):
    data = open(path, 'rb').read()
    if data[:2] == b'P6':
        parts = data.split(maxsplit=4)
        w, h = int(parts[1]), int(parts[2])
        raw = parts[4]
        return w, h, 3, raw
    # minimal PNG (8-bit RGB/RGBA, non-interlaced)
    assert data[:8] == b'\x89PNG\r\n\x1a\n'
    pos = 8; idat = b''; w = h = 0; ct = 0
    while pos < len(data):
        ln = struct.unpack('>I', data[pos:pos+4])[0]; typ = data[pos+4:pos+8]; body = data[pos+8:pos+8+ln]
        if typ == b'IHDR': w, h, bd, ct = struct.unpack('>IIBB', body[:10])
        elif typ == b'IDAT': idat += body
        pos += 12 + ln
    ch = {2: 3, 6: 4}[ct]
    raw = zlib.decompress(idat); stride = w * ch; out = bytearray(); prev = bytearray(stride); p = 0
    for _ in range(h):
        f = raw[p]; line = bytearray(raw[p+1:p+1+stride]); p += 1 + stride
        for i in range(stride):
            a = line[i-ch] if i >= ch else 0; b = prev[i]; c = prev[i-ch] if i >= ch else 0
            if f == 1: line[i] = (line[i] + a) & 255
            elif f == 2: line[i] = (line[i] + b) & 255
            elif f == 3: line[i] = (line[i] + (a + b) // 2) & 255
            elif f == 4:
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                pr = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 255
        out += line; prev = line
    return w, h, ch, bytes(out)

w, h, ch, raw = load(sys.argv[1])
rows = ['A', 'B', 'C', 'D', 'E']; counts = [2, 5, 3, 2, 3]
for r, n in enumerate(counts):
    y = r * 40 + 20
    cells = []
    for i in range(n):
        x = i * 40 + 20
        o = (y * w + x) * ch
        cells.append('#%02x%02x%02x' % tuple(raw[o:o+3]))
    print(rows[r], ' '.join(cells))
