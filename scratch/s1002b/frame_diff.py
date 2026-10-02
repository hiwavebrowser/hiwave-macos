"""Where two saved A/B frames differ: bounding box, pixel count, and a stacked crop (first above second).
usage: frame_diff.py <site> <armX> <armY> [out.png]   (frames are scratch/s1001g/frames/<site>-<arm>.png)"""
import sys
from PIL import Image, ImageChops
FR = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001g/frames'
site, x, y = sys.argv[1:4]
a = Image.open(f'{FR}/{site}-{x}.png').convert('RGB')
b = Image.open(f'{FR}/{site}-{y}.png').convert('RGB')
d = ImageChops.difference(a, b).convert('L').point(lambda p: 255 if p > 8 else 0)
box = d.getbbox()
print(site, x, y, 'differ in', box, 'pixels', sum(d.histogram()[255:]))
if box and len(sys.argv) > 4:
    x0, y0, x1, y1 = max(box[0] - 10, 0), max(box[1] - 10, 0), min(box[2] + 10, a.width), min(box[3] + 10, a.height)
    w, h = x1 - x0, y1 - y0
    sheet = Image.new('RGB', (w, h * 2 + 4), (255, 0, 255))
    sheet.paste(a.crop((x0, y0, x1, y1)), (0, 0))
    sheet.paste(b.crop((x0, y0, x1, y1)), (0, h + 4))
    sheet.save(sys.argv[4])
    print('->', sys.argv[4], sheet.size)
