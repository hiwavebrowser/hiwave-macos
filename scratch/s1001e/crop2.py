"""Stack crops of several frames (same box), each scaled, with a label strip between.
usage: crop2.py <out.png> <x0> <y0> <x1> <y1> <scale> <img> [...]"""
import sys
from PIL import Image
out = sys.argv[1]
x0, y0, x1, y1, scale = map(int, sys.argv[2:7])
crops = [Image.open(p).convert('RGB').crop((x0, y0, x1, y1)).resize(((x1 - x0) * scale, (y1 - y0) * scale), Image.LANCZOS)
         for p in sys.argv[7:]]
w = max(c.size[0] for c in crops)
h = sum(c.size[1] for c in crops) + 6 * (len(crops) - 1)
sheet = Image.new('RGB', (w, h), (255, 0, 0))
y = 0
for c in crops:
    sheet.paste(c, (0, y))
    y += c.size[1] + 6
sheet.save(out)
print(out, sheet.size)
