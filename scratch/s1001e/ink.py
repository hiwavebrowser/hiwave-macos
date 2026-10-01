"""Ink statistics of a box in several frames: darkest pixel, mean darkness, count of dark pixels.
usage: ink.py <x0> <y0> <x1> <y1> <img> [...]"""
import sys
from PIL import Image
x0, y0, x1, y1 = map(int, sys.argv[1:5])
for p in sys.argv[5:]:
    im = Image.open(p).convert('L').crop((x0, y0, x1, y1))
    px = list(im.getdata())
    ink = [255 - v for v in px]
    print(f'{p.split("/")[-1]:28} darkest {min(px):3}  ink sum {sum(ink):7}  px<128: {sum(1 for v in px if v < 128):4}  '
          f'px<64: {sum(1 for v in px if v < 64):4}')
