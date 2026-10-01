"""Pixel diff of the fixture's top 340 rows: develop vs Chrome, fix vs Chrome (threshold 8/255 per channel max),
and a stacked comparison image. usage: fixdiff.py"""
import os
from PIL import Image, ImageChops
D = os.path.dirname(os.path.abspath(__file__))
box = (0, 0, 1280, 340)
c = Image.open(D + '/chrome.png').convert('RGB').crop(box)
out = Image.new('RGB', (1280, 340 * 3), 'white')
for i, name in enumerate(('dev', 'fix')):
    im = Image.open(f'{D}/{name}.png').convert('RGB').crop(box)
    d = ImageChops.difference(im, c)
    n = sum(1 for p in d.getdata() if max(p) > 8)
    print(f'{name}: {n} px differ from Chrome = {100.0 * n / (1280 * 340):.3f}% of the 1280x340 region')
    out.paste(im, (0, 340 * i))
out.paste(c, (0, 680))
out.crop((0, 0, 1000, 1020)).save(D + '/dev-fix-chrome.png')
