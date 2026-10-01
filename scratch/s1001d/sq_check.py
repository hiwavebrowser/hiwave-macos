"""Check claims about the fixture frames: the square shadow box (row 1, sixth) is identical between arms;
print the per-box differing pixel counts (dev vs fix, fix vs chrome). usage: sq_check.py"""
import os
from PIL import Image, ImageChops
D = os.path.dirname(os.path.abspath(__file__))
dev, fix, chrome = (Image.open(f'{D}/{n}.png').convert('RGB') for n in ('dev', 'fix', 'chrome'))


def n(a, b, box):
    d = ImageChops.difference(a.crop(box), b.crop(box))
    return sum(1 for p in d.get_flattened_data() if max(p) > 8) if hasattr(d, 'get_flattened_data') else \
        sum(1 for p in d.getdata() if max(p) > 8)


# Row 1 boxes start at x=40, 180px apart (140 + gap 40), y=30; row 2 at y about 150, 160px apart.
for i in range(6):
    box = (40 + 180 * i - 25, 5, 40 + 180 * i + 165, 140)
    print(f'row1 box{i + 1}: dev-vs-fix {n(dev, fix, box):5}  dev-vs-chrome {n(dev, chrome, box):5}  fix-vs-chrome {n(fix, chrome, box):5}')
for i in range(6):
    box = (40 + 160 * i - 5, 145, 40 + 160 * i + 155, 300)
    print(f'row2 box{i + 1}: dev-vs-fix {n(dev, fix, box):5}  dev-vs-chrome {n(dev, chrome, box):5}  fix-vs-chrome {n(fix, chrome, box):5}')
