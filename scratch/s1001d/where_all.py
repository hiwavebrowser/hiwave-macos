"""For each site, where the develop (A1) and fix (B1) frames differ: bbox, count, and a side-by-side 2x crop.
usage: where_all.py <name> [...]"""
import sys
from PIL import Image, ImageChops
F = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001b/frames'
OUT = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001d'
for name in sys.argv[1:]:
    a = Image.open(f'{F}/{name}-A1.png').convert('RGB')
    b = Image.open(f'{F}/{name}-B1.png').convert('RGB')
    d = ImageChops.difference(a, b).convert('L').point(lambda p: 255 if p > 8 else 0)
    box = d.getbbox()
    n = sum(1 for p in d.tobytes() if p)
    print(name, 'bbox', box, 'changed px', n)
    if not box:
        continue
    x0, y0, x1, y1 = box
    x0, y0, x1, y1 = max(0, x0 - 10), max(0, y0 - 10), min(1280, x1 + 10), min(800, min(y1 + 10, y0 + 300))
    w, h = x1 - x0, y1 - y0
    out = Image.new('RGB', (w, h * 2 + 4), 'magenta')
    out.paste(a.crop((x0, y0, x1, y1)), (0, 0))
    out.paste(b.crop((x0, y0, x1, y1)), (0, h + 4))
    out.save(f'{OUT}/ab-{name}.png')
