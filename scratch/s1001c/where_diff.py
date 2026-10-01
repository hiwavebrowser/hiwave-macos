"""Where two frames differ: bounding box plus the runs of rows and columns holding changed pixels,
and a 3x crop of the first changed region side by side. usage: where_diff.py <a.png> <b.png> <out.png>"""
import sys
from PIL import Image, ImageChops
a, b = Image.open(sys.argv[1]).convert('RGB'), Image.open(sys.argv[2]).convert('RGB')
d = ImageChops.difference(a, b).convert('L').point(lambda p: 255 if p > 8 else 0)
box = d.getbbox()
print('bbox', box, 'changed px', sum(1 for p in d.tobytes() if p))
if not box:
    sys.exit(0)
px = d.load()
w, h = d.size


def runs(v):
    out, s, p = [], None, None
    for i in v:
        if s is None:
            s = p = i
        elif i <= p + 3:
            p = i
        else:
            out.append((s, p))
            s = p = i
    if s is not None:
        out.append((s, p))
    return out


rows = runs([y for y in range(h) if any(px[x, y] for x in range(w))])
print('rows', rows[:14])
y0, y1 = rows[0]
cols = runs([x for x in range(w) if any(px[x, y] for y in range(y0, y1 + 1))])
print('cols in first row run', cols[:14])
x0, x1 = cols[0]
crop = (max(0, x0 - 12), max(0, y0 - 12), min(w, x1 + 13), min(h, y1 + 13))
ca, cb = a.crop(crop), b.crop(crop)
out = Image.new('RGB', (ca.size[0] * 2 + 4, ca.size[1]), 'red')
out.paste(ca, (0, 0))
out.paste(cb, (ca.size[0] + 4, 0))
out = out.resize((out.size[0] * 3, out.size[1] * 3), Image.NEAREST)
out.save(sys.argv[3])
print('crop', crop, '->', sys.argv[3], out.size)
