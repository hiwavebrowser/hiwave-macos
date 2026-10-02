"""Capture one campaign case on two binaries and say where the frames differ (bounding box, pixel count),
saving a 2x crop of both, develop above fix.
usage: case_ab.py <binA> <binB> <case>"""
import glob, subprocess, sys
from PIL import Image, ImageChops
SRC = '/Users/petecopeland/Repos/.worktrees/rs-dev-9f49a40'
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
a, b, case = sys.argv[1:4]
hits = (glob.glob(f'{SRC}/websuite/**/{case}/index.html', recursive=True)
        + glob.glob(f'{SRC}/websuite/**/{case}.html', recursive=True))
print(hits[0])
frames = []
for label, binp in (('A', a), ('B', b)):
    out = f'{HUB}/scratch/s1002b/{case}-{label}.ppm'
    r = subprocess.run([binp, '--html-file', hits[0], '--width', '1280', '--height', '800', '--dump-frame', out],
                       capture_output=True, text=True, timeout=170)
    frames.append(Image.open(out).convert('RGB'))
d = ImageChops.difference(*frames).convert('L').point(lambda p: 255 if p else 0)
box = d.getbbox()
print('differ in', box, 'pixels', sum(1 for p in d.getdata() if p))
if box:
    x0, y0, x1, y1 = max(box[0] - 20, 0), max(box[1] - 20, 0), min(box[2] + 20, 1280), min(box[3] + 20, 800)
    w, h = x1 - x0, y1 - y0
    sheet = Image.new('RGB', (w, h * 2 + 4), (255, 0, 255))
    sheet.paste(frames[0].crop((x0, y0, x1, y1)), (0, 0))
    sheet.paste(frames[1].crop((x0, y0, x1, y1)), (0, h + 4))
    sheet = sheet.resize((w * 2, (h * 2 + 4) * 2), Image.NEAREST)
    sheet.save(f'{HUB}/scratch/s1002b/{case}-ab.png')
    print('->', f'{HUB}/scratch/s1002b/{case}-ab.png', sheet.size)
