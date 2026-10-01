"""Third image of the fixture's second row (`border-radius` on the <img> itself): differing pixels vs Chrome for
#407's frame (fix.png) and the stacked branch's (imgr.png), and for the whole 1280x340 region. usage: imgr_check.py"""
import os
from PIL import Image, ImageChops
D = os.path.dirname(os.path.abspath(__file__))
chrome = Image.open(D + '/chrome.png').convert('RGB')


def n(name, box):
    d = ImageChops.difference(Image.open(f'{D}/{name}.png').convert('RGB').crop(box), chrome.crop(box))
    mask = d.convert('L').point(lambda p: 0)  # placeholder size
    r, g, b = d.split()
    worst = ImageChops.lighter(ImageChops.lighter(r, g), b)
    return sum(1 for p in worst.tobytes() if p > 8)


for label, box in (('img box', (40 + 160 * 2 - 5, 145, 40 + 160 * 2 + 155, 300)), ('region', (0, 0, 1280, 340))):
    print(label, {name: n(name, box) for name in ('fix', 'imgr')})
