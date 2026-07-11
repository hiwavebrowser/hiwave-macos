#!/usr/bin/env python3
"""IFC Slice B2 probe — mixed-inline-wrap.html rendered by parity-capture.

Checks the brief's probe contract on RustKit's own frame (self-consistency,
not Chrome parity). B2 owns the SPLIT-AFTER-SIBLING shape (<b>Go</b> + long
plain run): every visual line's ink group must align as a unit — center
midpoints at the container middle, right edges at the container edge, left
edges at the origin.

Rows 1-3 (long text INSIDE <b>) materialize a nested Inline box; per-line
alignment there happens against the inline box's own width, not the
container — that is the inline-FRAGMENTATION gap the brief's §7 defers, so
those rows report informationally and only assert that wrapping happens.
See trench/forensics/2026-07-11-ifc-b2-midline-split-BRIEF.md §6/§7.
"""
import sys

PPM = sys.argv[1] if len(sys.argv) > 1 else 'parity-tests/repro/mixed-inline-wrap.ppm'
TOL = 2.5  # px — subpixel advance rounding + AA edge

def read_ppm(path):
    data = open(path, 'rb').read()
    if not data.startswith(b'P6'):
        raise SystemExit('not a P6 ppm')
    fields, i = [], 2
    while len(fields) < 3:
        while i < len(data) and data[i:i+1].isspace():
            i += 1
        if data[i:i+1] == b'#':
            while data[i:i+1] != b'\n':
                i += 1
            continue
        j = i
        while not data[j:j+1].isspace():
            j += 1
        fields.append(int(data[i:j])); i = j
    i += 1
    w, h, _maxv = fields
    return w, h, data[i:]

w, h, px = read_ppm(PPM)

def rgb(x, y):
    o = 3 * (y * w + x)
    return px[o], px[o+1], px[o+2]

def is_ink(x, y):
    r, g, b = rgb(x, y)
    return r + g + b < 450  # text ink (#000/#333 + AA); excludes #ddd border, #888 labels

def is_border(x, y):
    r, g, b = rgb(x, y)
    return abs(r-221) < 12 and abs(g-221) < 12 and abs(b-221) < 12

# 1. Find the bordered rows by scanning column x=0 for border runs.
rows, y = [], 0
while y < h:
    if is_border(0, y):
        y0 = y
        while y < h and is_border(0, y):
            y += 1
        rows.append((y0, y))
    else:
        y += 1
rows = [(a, b) for a, b in rows if b - a > 12]
assert len(rows) == 6, f'expected 6 bordered rows fully in frame, found {len(rows)}: {rows}'

# 2. Within each row, group ink scanlines into visual line bands (inner x 2..180).
def line_bands(y0, y1):
    bands, cur = [], None
    for yy in range(y0 + 1, y1 - 1):
        xs = [x for x in range(2, 180) if is_ink(x, yy)]
        if xs:
            if cur is None:
                cur = [min(xs), max(xs)]
            else:
                cur[0] = min(cur[0], min(xs)); cur[1] = max(cur[1], max(xs))
        elif cur is not None:
            bands.append(tuple(cur)); cur = None
    if cur is not None:
        bands.append(tuple(cur))
    return bands

# (name, align, hard) — hard=False: nested-Inline rows (fragmentation gap,
# deferred); hard=True: the B2 split-after-sibling rows.
CASES = [
    ('nested-inline center', 'center', False),
    ('nested-inline right', 'right', False),
    ('nested-inline left (phase-5 guard)', 'left', False),
    ('B2 center (<b>Go</b> + long plain)', 'center', True),
    ('B2 right (same tree)', 'right', True),
    ('B2 left (same tree, flow guard)', 'left', True),
]
failures = 0
for (y0, y1), (name, align, hard) in zip(rows, CASES):
    bands = line_bands(y0, y1)
    print(f'{name}: {len(bands)} visual lines')
    if len(bands) < 2:
        print('  FAIL: expected >=2 visual lines'); failures += 1
        continue
    for k, (x0, x1) in enumerate(bands):
        mid = (x0 + x1) / 2
        if align == 'center':
            ok = abs(mid - 91.0) <= TOL
            detail = f'mid={mid:.1f} (want 91±{TOL})'
        elif align == 'right':
            ok = abs(x1 - 179.0) <= TOL
            detail = f'right={x1} (want 179±{TOL})'
        else:
            # Left: line 0 starts at the origin; a mid-line LAST fragment
            # also returns to the origin. All bands here start at ~2.
            ok = abs(x0 - 2.0) <= TOL
            detail = f'left={x0} (want 2±{TOL})'
        status = 'ok' if ok else ('FAIL' if hard else 'info-miss (fragmentation gap)')
        print(f'  line {k}: ink [{x0},{x1}] {detail} {status}')
        if hard and not ok:
            failures += 1

print('PROBE', 'PASS' if failures == 0 else f'FAIL ({failures})')
sys.exit(0 if failures == 0 else 1)
